//! LF2 → element/ conversion layer.
//!
//! As of sub-checkpoint 3.2, this layer walks down to modules with
//! resolved dotted-name paths and feature flags. Modules contain no
//! data types yet — those land in 3.3 (records / variants / enums /
//! type-syns), 3.4 (types), 3.5 (templates), 3.6 (interfaces), 3.7
//! (exceptions), and 3.8 (values / expressions).

mod archive_payload;
mod data_payload;
mod interned;
mod module_payload;
mod package_payload;
mod util;

use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;

use bounded_static::ToBoundedStatic;

use crate::convert::archive_payload::DamlArchivePayload;
use crate::convert::data_payload::DamlDataPayload;
use crate::convert::interned::PackageInternedResolver;
use crate::convert::module_payload::DamlModulePayload;
use crate::convert::package_payload::DamlPackagePayload;
use crate::element::{DamlArchive, DamlData, DamlEnum, DamlFeatureFlags, DamlModule, DamlPackage, DamlRecord, DamlVariant};
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
    let leaf = DamlModule::new_leaf(
        leaf_path,
        DamlFeatureFlags::new(
            module.flags.forbid_party_literals,
            module.flags.dont_divulge_contract_ids_in_create_arguments,
            module.flags.dont_disclose_non_consuming_choices_to_observers,
        ),
        Vec::new(),
        data_types,
        #[cfg(feature = "full")]
        HashMap::new(),
    );
    cursor.take_from(leaf);
    Ok(())
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
    let mut out = HashMap::new();
    for payload in module.data_types() {
        if let Some(data) = build_data_type(payload, package, module_path)? {
            out.insert(data_key(&data), data);
        }
    }
    Ok(out)
}

fn build_data_type<'a>(
    payload: DamlDataPayload<'a>,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
) -> DamlLfResult<Option<DamlData<'a>>> {
    let path = package.resolve_dotted(payload.name_index())?;
    let name = path
        .last()
        .copied()
        .ok_or_else(|| crate::error::DamlLfConvertError::MissingRequiredField)?;
    let name_cow = Cow::Borrowed(name);
    let package_id_cow = Cow::Borrowed(package.package_id);
    let module_path_owned = module_path.to_vec();
    let data = match payload.data_cons() {
        Some(DataCons::Record(_)) => DamlData::Record(DamlRecord::new(
            name_cow,
            package_id_cow,
            module_path_owned,
            Vec::new(), // fields wired up in 3.4 once DamlType lands
            Vec::new(), // type params likewise
            payload.serializable(),
        )),
        Some(DataCons::Variant(_)) => DamlData::Variant(DamlVariant::new(
            name_cow,
            package_id_cow,
            module_path_owned,
            Vec::new(),
            Vec::new(),
            payload.serializable(),
        )),
        Some(DataCons::Enum(ec)) => {
            let constructors: Vec<Cow<'a, str>> =
                package.resolve_strings(&ec.constructors_interned_str)?.into_iter().map(Cow::Borrowed).collect();
            DamlData::Enum(DamlEnum::new(
                name_cow,
                package_id_cow,
                module_path_owned,
                constructors,
                Vec::new(),
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
