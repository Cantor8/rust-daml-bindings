//! LF2 → element/ conversion layer.
//!
//! As of sub-checkpoint 3.4, the layer covers modules, data types
//! (records / variants / enums with their fields and type parameters
//! fully populated), and type synonyms. Templates land in 3.5,
//! interfaces in 3.6, exceptions in 3.7, and values + expressions
//! in 3.8.

mod archive_payload;
mod data_payload;
mod exception_payload;
mod field_payload;
mod interface_payload;
mod interned;
mod module_payload;
mod package_payload;
mod template_payload;
mod type_payload;
mod typevar_payload;
mod util;

use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;

use bounded_static::ToBoundedStatic;

use crate::convert::archive_payload::DamlArchivePayload;
use crate::convert::data_payload::DamlDataPayload;
use crate::convert::exception_payload::convert_exception;
use crate::convert::field_payload::convert_field;
use crate::convert::interface_payload::convert_interface;
use crate::convert::interned::PackageInternedResolver;
use crate::convert::module_payload::DamlModulePayload;
use crate::convert::package_payload::DamlPackagePayload;
use crate::convert::template_payload::{convert_choice, convert_def_key, convert_implements};
use crate::convert::type_payload::{convert_type, convert_type_params};
use crate::element::{
    DamlArchive, DamlData, DamlDefTypeSyn, DamlEnum, DamlException, DamlFeatureFlags, DamlInterface, DamlModule,
    DamlPackage, DamlRecord, DamlTemplate, DamlVariant,
};
use crate::lf_protobuf::daml_lf_2;
use crate::lf_protobuf::daml_lf_2::def_data_type::DataCons;
use crate::{DamlLfArchive, DamlLfArchivePayload, DamlLfHashFunction, DamlLfResult, DarFile};

/// Create an owned [`DamlArchive`] from a [`DarFile`].
pub fn to_owned_archive(dar: &DarFile) -> DamlLfResult<DamlArchive<'static>> {
    apply_dar(dar, |archive| archive.to_static())
}

/// Convert a [`DarFile`] to a [`DamlArchive`] and map `f` over it.
pub fn apply_dar<R, F>(dar: &DarFile, f: F) -> DamlLfResult<R>
where
    F: FnOnce(&DamlArchive<'_>) -> R,
{
    let payload = DamlArchivePayload::try_from(dar)?;
    let archive = build_archive(&payload)?;
    Ok(f(&archive))
}

/// Create a [`DamlArchive`] from a [`DamlLfArchive`] and apply it to `f`.
pub fn apply_dalf<R, F>(dalf: &DamlLfArchive, f: F) -> DamlLfResult<R>
where
    F: FnOnce(&DamlPackage<'_>) -> R,
{
    let package_payload = DamlPackagePayload::try_from(dalf)?;
    let payload = DamlArchivePayload::from_single_package(package_payload);
    let archive = build_archive(&payload)?;
    let package = archive.packages().next().expect("single-package archive must have one package");
    Ok(f(package))
}

/// Create a [`DamlArchive`] from a [`DamlLfArchivePayload`] and apply it to `f`.
pub fn apply_payload<R, F>(payload: DamlLfArchivePayload, f: F) -> DamlLfResult<R>
where
    F: FnOnce(&DamlPackage<'_>) -> R,
{
    let dalf = DamlLfArchive::new("unnamed", payload, DamlLfHashFunction::Sha256, "");
    apply_dalf(&dalf, f)
}

fn build_archive<'a>(payload: &'a DamlArchivePayload<'a>) -> DamlLfResult<DamlArchive<'a>> {
    let packages: HashMap<Cow<'a, str>, DamlPackage<'a>> = payload
        .packages
        .values()
        .map(|pkg| build_package(pkg).map(|p| (Cow::Borrowed(pkg.package_id), p)))
        .collect::<DamlLfResult<_>>()?;
    Ok(DamlArchive::new(
        Cow::Borrowed(payload.archive_name),
        Cow::Borrowed(payload.main_package_id),
        packages,
    ))
}

fn build_package<'a>(payload: &'a DamlPackagePayload<'a>) -> DamlLfResult<DamlPackage<'a>> {
    let root = build_module_tree(payload)?;
    Ok(DamlPackage::new(
        Cow::Borrowed(payload.name.as_str()),
        Cow::Borrowed(payload.package_id),
        payload.version.as_deref().map(Cow::Borrowed),
        payload.language_version,
        root,
    ))
}

/// Walk the package's flat list of modules and build a nested
/// [`DamlModule`] tree keyed by dotted-name segment. Each module's
/// feature flags, path, and data-type names are filled in; type
/// synonyms (and the type-system content of each data type) land
/// with 3.4; templates with 3.5; interfaces with 3.6; exceptions
/// with 3.7; values with 3.8.
fn build_module_tree<'a>(payload: &'a DamlPackagePayload<'a>) -> DamlLfResult<DamlModule<'a>> {
    let mut root = DamlModule::new_root();
    for module in &payload.modules {
        let path = module.path(payload)?;
        insert_module(&mut root, &path, module, payload)?;
    }
    Ok(root)
}

fn insert_module<'a>(
    root: &mut DamlModule<'a>,
    path: &[&'a str],
    module: &DamlModulePayload<'a>,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfResult<()> {
    let mut cursor = root;
    for segment in path {
        cursor = cursor.child_module_or_new(segment);
    }
    let leaf_path = path.iter().map(|s| Cow::Borrowed(*s)).collect::<Vec<_>>();
    let data_types = build_data_types(module, package, &leaf_path)?;
    let synonyms = build_synonyms(module, package, &leaf_path)?;
    let interfaces = build_interfaces(module, package, &leaf_path)?;
    let exceptions = build_exceptions(module, package, &leaf_path)?;
    let leaf = DamlModule::new_leaf(
        leaf_path,
        DamlFeatureFlags::new(
            module.flags.forbid_party_literals,
            module.flags.dont_divulge_contract_ids_in_create_arguments,
            module.flags.dont_disclose_non_consuming_choices_to_observers,
        ),
        synonyms,
        data_types,
        interfaces,
        exceptions,
        #[cfg(feature = "full")]
        HashMap::new(),
    );
    cursor.take_from(leaf);
    Ok(())
}

fn build_interfaces<'a>(
    module: &DamlModulePayload<'a>,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
) -> DamlLfResult<HashMap<Cow<'a, str>, DamlInterface<'a>>> {
    let mut out = HashMap::new();
    for proto in module.interfaces() {
        let interface = convert_interface(proto, package, module_path)?;
        out.insert(Cow::Owned(interface.name().to_owned()), interface);
    }
    Ok(out)
}

fn build_exceptions<'a>(
    module: &DamlModulePayload<'a>,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
) -> DamlLfResult<HashMap<Cow<'a, str>, DamlException<'a>>> {
    let mut out = HashMap::new();
    for proto in module.exceptions() {
        let exception = convert_exception(proto, package, module_path)?;
        out.insert(Cow::Owned(exception.name().to_owned()), exception);
    }
    Ok(out)
}

fn build_synonyms<'a>(
    module: &DamlModulePayload<'a>,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
) -> DamlLfResult<Vec<DamlDefTypeSyn<'a>>> {
    module
        .synonyms()
        .iter()
        .map(|syn| {
            let name_segments = package.resolve_dotted(syn.name_interned_dname)?;
            let name_cow: Vec<Cow<'a, str>> = module_path
                .iter()
                .cloned()
                .chain(name_segments.iter().copied().map(Cow::Borrowed))
                .collect();
            let params = convert_type_params(&syn.params, package)?;
            let ty = convert_type(
                syn.r#type.as_ref().ok_or(crate::error::DamlLfConvertError::MissingRequiredField)?,
                package,
            )?;
            Ok(DamlDefTypeSyn::new(params, ty, name_cow))
        })
        .collect()
}

/// Convert every `DefDataType` in `module` into a `DamlData` keyed
/// by its (final-segment) name.
///
/// 3.3 leaves field types, type-parameter kinds, and synonym bodies
/// empty — those need the [`crate::element::DamlType`] machinery
/// that 3.4 lands. Enum constructors *are* fully populated; they
/// carry interned strings only and don't need the type system.
///
/// DefDataType entries whose `data_cons` is the `Interface` marker
/// are filtered out — the actual interface definitions live on the
/// module's `interfaces` list and 3.6 will wire those into
/// `element/`.
fn build_data_types<'a>(
    module: &DamlModulePayload<'a>,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
) -> DamlLfResult<HashMap<Cow<'a, str>, DamlData<'a>>> {
    // Pre-index this module's templates by the data type they wrap
    // (templates and data-types are sibling lists in LF2; a template
    // references its argument record via `tycon_interned_dname`).
    let template_by_tycon: HashMap<i32, &daml_lf_2::DefTemplate> =
        module.templates().iter().map(|t| (t.tycon_interned_dname, t)).collect();
    let mut out = HashMap::new();
    for payload in module.data_types() {
        if let Some(data) = build_data_type(payload, package, module_path, &template_by_tycon)? {
            out.insert(data_key(&data), data);
        }
    }
    Ok(out)
}

fn build_data_type<'a>(
    payload: DamlDataPayload<'a>,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
    template_by_tycon: &HashMap<i32, &'a daml_lf_2::DefTemplate>,
) -> DamlLfResult<Option<DamlData<'a>>> {
    let path = package.resolve_dotted(payload.name_index())?;
    let name = path
        .last()
        .copied()
        .ok_or_else(|| crate::error::DamlLfConvertError::MissingRequiredField)?;
    let name_cow = Cow::Borrowed(name);
    let package_id_cow = Cow::Borrowed(package.package_id);
    let module_path_owned = module_path.to_vec();
    let type_params = convert_type_params(payload.params(), package)?;
    let data = match payload.data_cons() {
        Some(DataCons::Record(fields)) => {
            let daml_fields = fields
                .fields
                .iter()
                .map(|f| convert_field(f, package))
                .collect::<crate::error::DamlLfConvertResult<Vec<_>>>()?;
            // If this record is wrapped by a DefTemplate, surface
            // the richer DamlTemplate shape instead.
            if let Some(template) = template_by_tycon.get(&payload.name_index()) {
                DamlData::Template(Box::new(build_template(
                    template,
                    name_cow,
                    package_id_cow,
                    module_path_owned,
                    daml_fields,
                    package,
                    payload.serializable(),
                )?))
            } else {
                DamlData::Record(DamlRecord::new(
                    name_cow,
                    package_id_cow,
                    module_path_owned,
                    daml_fields,
                    type_params,
                    payload.serializable(),
                ))
            }
        },
        Some(DataCons::Variant(fields)) => {
            let daml_fields = fields
                .fields
                .iter()
                .map(|f| convert_field(f, package))
                .collect::<crate::error::DamlLfConvertResult<_>>()?;
            DamlData::Variant(DamlVariant::new(
                name_cow,
                package_id_cow,
                module_path_owned,
                daml_fields,
                type_params,
                payload.serializable(),
            ))
        },
        Some(DataCons::Enum(ec)) => {
            let constructors: Vec<Cow<'a, str>> =
                package.resolve_strings(&ec.constructors_interned_str)?.into_iter().map(Cow::Borrowed).collect();
            DamlData::Enum(DamlEnum::new(
                name_cow,
                package_id_cow,
                module_path_owned,
                constructors,
                type_params,
                payload.serializable(),
            ))
        },
        // Interface marker — DefInterface carries the real surface; 3.6 wires it up.
        Some(DataCons::Interface(_)) => return Ok(None),
        None => return Err(crate::error::DamlLfConvertError::MissingRequiredField.into()),
    };
    Ok(Some(data))
}

/// Pull the (final) dotted-name segment as the key under which a
/// DamlData lives in its module's `data_types` map. The element
/// layer keys by single name, not by full path, so we strip the
/// module prefix.
fn data_key<'a>(data: &DamlData<'a>) -> Cow<'a, str> {
    Cow::Owned(data.name().to_owned())
}

/// Combine the record-shaped fields a DefDataType supplies with the
/// template-only metadata (choices, key, param, implements) into a
/// [`DamlTemplate`]. Expr-typed fields (precond, signatories,
/// agreement, observers) live behind `#[cfg(feature = "full")]` and
/// land in 3.8.
#[allow(clippy::too_many_arguments)]
fn build_template<'a>(
    template: &'a daml_lf_2::DefTemplate,
    name: Cow<'a, str>,
    package_id: Cow<'a, str>,
    module_path: Vec<Cow<'a, str>>,
    fields: Vec<crate::element::DamlField<'a>>,
    package: &'a DamlPackagePayload<'a>,
    serializable: bool,
) -> DamlLfResult<DamlTemplate<'a>> {
    let param = package.resolve_string(template.param_interned_str)?;
    let choices: Vec<_> = template
        .choices
        .iter()
        .map(|c| convert_choice(c, package, &module_path, &package_id))
        .collect::<crate::error::DamlLfConvertResult<_>>()?;
    let key = template.key.as_ref().map(|k| convert_def_key(k, package)).transpose()?;
    let implements = convert_implements(&template.implements, package)?;
    Ok(DamlTemplate::new(
        name,
        package_id,
        module_path,
        fields,
        choices,
        Cow::Borrowed(param),
        implements,
        key,
        serializable,
    ))
}
