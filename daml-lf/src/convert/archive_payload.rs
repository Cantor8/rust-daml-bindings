use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;

use crate::DarFile;
use crate::convert::package_payload::DamlPackagePayload;
use crate::error::{DamlLfConvertError, DamlLfConvertResult};

/// Borrowed view of all packages in a [`DarFile`], indexed by
/// package-id, with the main package's id called out separately.
///
/// Holds references into the [`DarFile`], so its lifetime is bounded
/// by the dar's.
#[derive(Debug)]
pub struct DamlArchivePayload<'a> {
    pub archive_name: &'a str,
    pub main_package_id: &'a str,
    pub packages: HashMap<&'a str, DamlPackagePayload<'a>>,
}

impl<'a> DamlArchivePayload<'a> {
    /// Single-package archive — used by `apply_dalf` /
    /// `apply_payload` paths that don't carry the full DAR
    /// structure.
    pub fn from_single_package(package: DamlPackagePayload<'a>) -> Self {
        let package_id = package.package_id;
        let mut packages = HashMap::new();
        packages.insert(package_id, package);
        Self {
            archive_name: "unnamed",
            main_package_id: package_id,
            packages,
        }
    }

    /// Look up a package by id; useful when 2.dev `PackageImportId`
    /// resolution lands (currently unused but inexpensive to keep).
    #[allow(dead_code)]
    pub fn package_by_id(&self, package_id: &str) -> Option<&DamlPackagePayload<'a>> {
        self.packages.get(package_id)
    }
}

impl<'a> TryFrom<&'a DarFile> for DamlArchivePayload<'a> {
    type Error = DamlLfConvertError;

    fn try_from(dar: &'a DarFile) -> DamlLfConvertResult<Self> {
        let mut packages = HashMap::with_capacity(dar.dependencies.len() + 1);
        let main = DamlPackagePayload::try_from(&dar.main)?;
        packages.insert(main.package_id, main);
        for dep in &dar.dependencies {
            let dep_payload = DamlPackagePayload::try_from(dep)?;
            packages.insert(dep_payload.package_id, dep_payload);
        }
        // Build the cross-package name table once and stamp each
        // package with a shared `Arc` of it, so `convert_tycon_id`
        // can resolve cross-package references without threading
        // the archive through every helper. Note: `package.name`
        // is owned; the table is independent of the underlying
        // `DarFile`'s lifetime.
        let names_table: HashMap<String, String> =
            packages.iter().map(|(id, pkg)| ((*id).to_owned(), pkg.name.clone())).collect();
        let shared = Arc::new(names_table);
        for pkg in packages.values_mut() {
            pkg.set_pkg_names(shared.clone());
        }
        Ok(Self {
            archive_name: dar.main.name.as_str(),
            main_package_id: dar.main.hash.as_str(),
            packages,
        })
    }
}
