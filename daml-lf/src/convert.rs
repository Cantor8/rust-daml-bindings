//! LF2 → element/ conversion layer.
//!
//! This file currently holds stubs: the public functions build empty
//! [`DamlArchive`] / [`DamlPackage`] values from the loaded metadata
//! (package id, language version) and don't yet walk the LF2 message
//! tree to populate modules, data types, templates, etc.
//!
//! Subsequent Phase 3 sub-checkpoints (3.2 onwards) fill in the
//! conversion piece by piece: package + module scaffolding, then
//! records / variants / enums / type-syns, then the type system,
//! then templates + choices + keys, then interfaces, then
//! exceptions, then (under `feature = "full"`) values and the
//! expression tree.
//!
//! The empty-archive shape is intentional — the rest of the crate
//! (`DarFile::from_file`, the public API surface, the LF2 envelope
//! decoding in `payload.rs`) needs a compiling target during the
//! incremental rewrite. End-to-end DAR parsing will start returning
//! real content at 3.2.

use std::borrow::Cow;
use std::collections::HashMap;

use bounded_static::ToBoundedStatic;

use crate::element::{DamlArchive, DamlModule, DamlPackage};
use crate::{DamlLfArchive, DamlLfArchivePayload, DamlLfHashFunction, DamlLfResult, DarFile};

/// Create an owned [`DamlArchive`] from a [`DarFile`].
///
/// Currently returns an archive whose packages all have empty modules
/// — see the module-level doc.
pub fn to_owned_archive(dar: &DarFile) -> DamlLfResult<DamlArchive<'static>> {
    apply_dar(dar, |archive| archive.to_static())
}

/// Convert a [`DarFile`] to a [`DamlArchive`] and map `f` over it.
///
/// Currently the archive contains the dar's packages with metadata
/// (id, name, version, language version) populated but with empty
/// module trees — see the module-level doc.
pub fn apply_dar<R, F>(dar: &DarFile, f: F) -> DamlLfResult<R>
where
    F: FnOnce(&DamlArchive<'_>) -> R,
{
    let archive = build_skeleton_archive(dar);
    Ok(f(&archive))
}

/// Create a [`DamlArchive`] from a [`DamlLfArchive`] and apply it to `f`.
pub fn apply_dalf<R, F>(dalf: &DamlLfArchive, f: F) -> DamlLfResult<R>
where
    F: FnOnce(&DamlPackage<'_>) -> R,
{
    let package = build_skeleton_package(dalf);
    Ok(f(&package))
}

/// Create a [`DamlArchive`] from a [`DamlLfArchivePayload`] and apply
/// it to `f`. The payload alone doesn't carry an `archive name` or a
/// `hash`, so the stub fills those with placeholder values; once the
/// conversion is real, callers that care about identity should go
/// through [`apply_dalf`] or [`apply_dar`].
pub fn apply_payload<R, F>(payload: DamlLfArchivePayload, f: F) -> DamlLfResult<R>
where
    F: FnOnce(&DamlPackage<'_>) -> R,
{
    let dalf = DamlLfArchive::new("unnamed", payload, DamlLfHashFunction::Sha256, "");
    let package = build_skeleton_package(&dalf);
    Ok(f(&package))
}

fn build_skeleton_archive(dar: &DarFile) -> DamlArchive<'_> {
    let main_package_id: Cow<'_, str> = Cow::Borrowed(&dar.main.hash);
    let mut packages: HashMap<Cow<'_, str>, DamlPackage<'_>> = HashMap::new();
    packages.insert(Cow::Borrowed(&dar.main.hash), build_skeleton_package(&dar.main));
    for dalf in &dar.dependencies {
        packages.insert(Cow::Borrowed(&dalf.hash), build_skeleton_package(dalf));
    }
    DamlArchive::new(Cow::Borrowed(&dar.main.name), main_package_id, packages)
}

fn build_skeleton_package(dalf: &DamlLfArchive) -> DamlPackage<'_> {
    DamlPackage::new(
        Cow::Borrowed(&dalf.name),
        Cow::Borrowed(&dalf.hash),
        None, // package version is parsed from package metadata at 3.2
        dalf.payload.language_version,
        DamlModule::new_root(),
    )
}
