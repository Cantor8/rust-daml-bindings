//! LF2 → element/ conversion layer.
//!
//! As of sub-checkpoint 3.2, this layer walks down to modules with
//! resolved dotted-name paths and feature flags. Modules contain no
//! data types yet — those land in 3.3 (records / variants / enums /
//! type-syns), 3.4 (types), 3.5 (templates), 3.6 (interfaces), 3.7
//! (exceptions), and 3.8 (values / expressions).

mod archive_payload;
mod interned;
mod module_payload;
mod package_payload;
mod util;

use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;

use bounded_static::ToBoundedStatic;

use crate::convert::archive_payload::DamlArchivePayload;
use crate::convert::interned::PackageInternedResolver;
use crate::convert::module_payload::DamlModulePayload;
use crate::convert::package_payload::DamlPackagePayload;
use crate::element::{DamlArchive, DamlFeatureFlags, DamlModule, DamlPackage};
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
/// feature flags and path are filled in; everything below
/// (data_types, synonyms, values, …) stays empty until the
/// corresponding sub-checkpoint lands.
fn build_module_tree<'a>(payload: &'a DamlPackagePayload<'a>) -> DamlLfResult<DamlModule<'a>> {
    let mut root = DamlModule::new_root();
    for module in &payload.modules {
        let path = module.path(payload)?;
        insert_module(&mut root, &path, module);
    }
    Ok(root)
}

fn insert_module<'a>(root: &mut DamlModule<'a>, path: &[&'a str], payload: &DamlModulePayload<'a>) {
    let mut cursor = root;
    for segment in path {
        cursor = cursor.child_module_or_new(segment);
    }
    let leaf_path = path.iter().map(|s| Cow::Borrowed(*s)).collect::<Vec<_>>();
    let leaf = DamlModule::new_leaf(
        leaf_path,
        DamlFeatureFlags::new(
            payload.flags.forbid_party_literals,
            payload.flags.dont_divulge_contract_ids_in_create_arguments,
            payload.flags.dont_disclose_non_consuming_choices_to_observers,
        ),
        Vec::new(),
        HashMap::new(),
        #[cfg(feature = "full")]
        HashMap::new(),
    );
    cursor.take_from(leaf);
}
