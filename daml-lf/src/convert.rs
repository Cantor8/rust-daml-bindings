//! LF2 → element/ conversion layer.

mod archive_payload;
mod data_payload;
#[cfg(feature = "full")]
mod defvalue_payload;
mod exception_payload;
#[cfg(feature = "full")]
mod expr_payload;
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
#[cfg(feature = "full")]
use crate::convert::defvalue_payload::convert_def_value;
use crate::convert::exception_payload::convert_exception;
#[cfg(feature = "full")]
use crate::convert::expr_payload::convert_expr;
use crate::convert::field_payload::convert_field;
use crate::convert::interface_payload::convert_interface;
use crate::convert::interned::PackageInternedResolver;
use crate::convert::module_payload::DamlModulePayload;
use crate::convert::package_payload::DamlPackagePayload;
use crate::convert::template_payload::{convert_choice, convert_def_key, convert_implements};
use crate::convert::type_payload::{convert_type, convert_type_params};
#[cfg(feature = "full")]
use crate::element::DamlDefValue;
use crate::element::{
    DamlArchive, DamlData, DamlDefTypeSyn, DamlEnum, DamlException, DamlFeatureFlags, DamlInterface, DamlModule,
    DamlPackage, DamlRecord, DamlTemplate, DamlVariant,
};
use crate::error::DamlLfConvertError;
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
    Ok(DamlArchive::new(Cow::Borrowed(payload.archive_name), Cow::Borrowed(payload.main_package_id), packages))
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
/// feature flags, path, and data-type names are filled in;
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
    // LF2 mandates the safer defaults for all three transitional LF1 feature flags
    // (they exist on the wire only for backward compatibility with LF1 archives,
    // which this crate no longer loads). Reject any LF2 module that opts back into
    // the LF1-era loose semantics — that combination is not a valid LF2 archive.
    let lf_ver = package.language_version().to_string();
    if !module.flags.forbid_party_literals {
        return Err(DamlLfConvertError::UnsupportedFeatureUsed(
            lf_ver,
            "party-literal syntax (LF1-only; use the Party type instead)".into(),
            "LF1".into(),
        )
        .into());
    }
    if !module.flags.dont_divulge_contract_ids_in_create_arguments {
        return Err(DamlLfConvertError::UnsupportedFeatureUsed(
            lf_ver,
            "contract-id divulgence in create arguments (LF1-only)".into(),
            "LF1".into(),
        )
        .into());
    }
    if !module.flags.dont_disclose_non_consuming_choices_to_observers {
        return Err(DamlLfConvertError::UnsupportedFeatureUsed(
            lf_ver,
            "non-consuming choice disclosure to observers (LF1-only)".into(),
            "LF1".into(),
        )
        .into());
    }
    let (direct_data, nested_data) = build_data_types(module, package, &leaf_path)?;
    let synonyms = build_synonyms(module, package, &leaf_path)?;
    let interfaces = build_interfaces(module, package, &leaf_path)?;
    let exceptions = build_exceptions(module, package, &leaf_path)?;
    #[cfg(feature = "full")]
    let values = build_values(module, package)?;
    let leaf = DamlModule::new_leaf(
        leaf_path,
        DamlFeatureFlags::new(
            module.flags.forbid_party_literals,
            module.flags.dont_divulge_contract_ids_in_create_arguments,
            module.flags.dont_disclose_non_consuming_choices_to_observers,
        ),
        synonyms,
        direct_data,
        interfaces,
        exceptions,
        #[cfg(feature = "full")]
        values,
    );
    cursor.take_from(leaf);
    // Re-route variant-record-payload data (LF2 emits these as
    // dotted-name data types like `Shape.Circle`) into synthetic
    // child modules so the payload records sit at the same module
    // path that `convert_tycon_id` produces for references to them.
    // Without this, `data_by_tycon_name` lookups for cross-record
    // references inside a variant would miss the payload data and
    // codegen would emit `crate::...::shape::Circle` paths backed
    // by no actual sub-module.
    for (extra_path, key, data) in nested_data {
        let mut nested_cursor: &mut DamlModule<'a> = cursor;
        for segment in extra_path {
            nested_cursor = nested_cursor.synthetic_child_or_new(segment);
        }
        nested_cursor.insert_data_type(key, data);
    }
    Ok(())
}

#[cfg(feature = "full")]
fn build_values<'a>(
    module: &DamlModulePayload<'a>,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfResult<HashMap<Cow<'a, str>, DamlDefValue<'a>>> {
    let mut out = HashMap::new();
    for proto in module.values() {
        let value = convert_def_value(proto, package)?;
        out.insert(value.name_clone(), value);
    }
    Ok(out)
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
            let name_cow: Vec<Cow<'a, str>> =
                module_path.iter().cloned().chain(name_segments.iter().copied().map(Cow::Borrowed)).collect();
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
/// `DefDataType` entries whose `data_cons` is the `Interface` marker
/// are filtered out — the actual interface definitions live on the
/// module's `interfaces` list and `convert_interface` wires those
/// into `element/`.
/// Output of [`build_data_types`]: data that goes straight into
/// the LF module's own bucket, plus data routed into synthetic
/// child modules to mirror dotted-name prefixes (the
/// variant-record-payload case; see the call site in
/// [`insert_module`]).
type BuiltDataTypes<'a> = (HashMap<Cow<'a, str>, DamlData<'a>>, Vec<(Vec<&'a str>, Cow<'a, str>, DamlData<'a>)>);

fn build_data_types<'a>(
    module: &DamlModulePayload<'a>,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
) -> DamlLfResult<BuiltDataTypes<'a>> {
    // Pre-index this module's templates by the data type they wrap
    // (templates and data-types are sibling lists in LF2; a template
    // references its argument record via `tycon_interned_dname`).
    let template_by_tycon: HashMap<i32, &daml_lf_2::DefTemplate> =
        module.templates().iter().map(|t| (t.tycon_interned_dname, t)).collect();
    let mut direct = HashMap::new();
    let mut nested = Vec::new();
    for payload in module.data_types() {
        let path = package.resolve_dotted(payload.name_index())?;
        let (last_seg, prefix) = path.split_last().ok_or(crate::error::DamlLfConvertError::MissingRequiredField)?;
        // Fold the dotted-name prefix into the data's module path.
        // For top-level data (single-segment name) prefix is empty
        // and this collapses to just the module's own path. For
        // variant-record payloads (multi-segment, e.g.
        // `Shape.Circle`) the prefix carries the synthetic
        // sub-namespace.
        let full_module_path: Vec<Cow<'a, str>> =
            module_path.iter().cloned().chain(prefix.iter().copied().map(Cow::Borrowed)).collect();
        if let Some(data) = build_data_type(payload, last_seg, package, &full_module_path, &template_by_tycon)? {
            if prefix.is_empty() {
                direct.insert(data_key(&data), data);
            } else {
                nested.push((prefix.to_vec(), Cow::Borrowed(*last_seg), data));
            }
        }
    }
    Ok((direct, nested))
}

fn build_data_type<'a>(
    payload: DamlDataPayload<'a>,
    name: &'a str,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
    template_by_tycon: &HashMap<i32, &'a daml_lf_2::DefTemplate>,
) -> DamlLfResult<Option<DamlData<'a>>> {
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
        // Interface marker — DefInterface carries the real surface; wired up by convert_interface.
        Some(DataCons::Interface(_)) => return Ok(None),
        None => return Err(crate::error::DamlLfConvertError::MissingRequiredField.into()),
    };
    Ok(Some(data))
}

/// Pull the (final) dotted-name segment as the key under which a
/// `DamlData` lives in its module's `data_types` map. The element
/// layer keys by single name, not by full path, so we strip the
/// module prefix.
fn data_key<'a>(data: &DamlData<'a>) -> Cow<'a, str> {
    Cow::Owned(data.name().to_owned())
}

/// Combine the record-shaped fields a `DefDataType` supplies with the
/// template-only metadata (choices, key, param, implements) into a
/// [`DamlTemplate`]. Under `--features full`, the Expr-typed fields
/// (precond, signatories, observers) are populated from the LF2
/// proto; `agreement` was removed in LF2 so we substitute an empty
/// text placeholder (matching `DamlTemplate::new_with_defaults`).
#[allow(clippy::too_many_arguments)]
fn build_template<'a>(
    template: &'a daml_lf_2::DefTemplate,
    name: Cow<'a, str>,
    module_path: Vec<Cow<'a, str>>,
    fields: Vec<crate::element::DamlField<'a>>,
    package: &'a DamlPackagePayload<'a>,
    serializable: bool,
) -> DamlLfResult<DamlTemplate<'a>> {
    let package_id = Cow::Borrowed(package.package_id);
    let param = package.resolve_string(template.param_interned_str)?;
    let choices: Vec<_> = template
        .choices
        .iter()
        .map(|c| convert_choice(c, package, &module_path))
        .collect::<crate::error::DamlLfConvertResult<_>>()?;
    let key = template.key.as_ref().map(|k| convert_def_key(k, package)).transpose()?;
    let implements = convert_implements(&template.implements, package)?;
    #[cfg(feature = "full")]
    let precond = template.precond.as_ref().map(|p| convert_expr(p, package)).transpose()?;
    #[cfg(feature = "full")]
    let signatories = {
        use crate::convert::util::Required;
        convert_expr(template.signatories.as_ref().req()?, package)?
    };
    #[cfg(feature = "full")]
    let observers = {
        use crate::convert::util::Required;
        convert_expr(template.observers.as_ref().req()?, package)?
    };
    Ok(DamlTemplate::new(
        name,
        package_id,
        module_path,
        fields,
        choices,
        Cow::Borrowed(param),
        implements,
        #[cfg(feature = "full")]
        precond,
        #[cfg(feature = "full")]
        signatories,
        #[cfg(feature = "full")]
        agreement_placeholder(),
        #[cfg(feature = "full")]
        observers,
        key,
        serializable,
    ))
}

/// LF2 dropped the `agreement` field on `DefTemplate`. The element
/// layer still carries it for API stability; supply the same empty
/// text default used by [`DamlTemplate::new_with_defaults`].
#[cfg(feature = "full")]
fn agreement_placeholder<'a>() -> crate::element::DamlExpr<'a> {
    use crate::element::{DamlExpr, DamlPrimLit};
    DamlExpr::PrimLit(DamlPrimLit::Text(Cow::Borrowed("")))
}
