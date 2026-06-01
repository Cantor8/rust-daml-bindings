use std::convert::TryFrom;

use crate::convert::interned::PackageInternedResolver;
use crate::convert::module_payload::DamlModulePayload;
use crate::convert::util::Required;
use crate::error::{DamlLfConvertError, DamlLfConvertResult};
use crate::{DamlLfArchive, DamlLfPackage, LanguageVersion};

/// Borrowed view of an LF2 `Package`: metadata (name, version,
/// language version, package-id), the package-level interning
/// tables, and the list of [`DamlModulePayload`]s.
///
/// Holds references into the underlying [`DamlLfArchive`], so its
/// lifetime is bounded by the archive's.
#[derive(Debug)]
pub struct DamlPackagePayload<'a> {
    pub name: String,
    pub version: Option<String>,
    pub language_version: LanguageVersion,
    pub package_id: &'a str,
    interned_strings: &'a [String],
    interned_dotted_names: Vec<&'a [i32]>,
    pub modules: Vec<DamlModulePayload<'a>>,
}

impl<'a> PackageInternedResolver for DamlPackagePayload<'a> {
    fn package_id(&self) -> &str {
        self.package_id
    }

    fn interned_strings(&self) -> &[String] {
        self.interned_strings
    }

    fn interned_dotted_names(&self) -> &[&[i32]] {
        &self.interned_dotted_names
    }
}

impl<'a> TryFrom<&'a DamlLfArchive> for DamlPackagePayload<'a> {
    type Error = DamlLfConvertError;

    fn try_from(dalf: &'a DamlLfArchive) -> DamlLfConvertResult<Self> {
        let DamlLfPackage::V2(package) = &dalf.payload.package;
        let language_version = dalf.payload.language_version;
        let package_id = dalf.hash.as_str();
        let interned_strings = package.interned_strings.as_slice();
        let interned_dotted_names: Vec<&[i32]> =
            package.interned_dotted_names.iter().map(|dn| dn.segments_interned_str.as_slice()).collect();
        let metadata = package.metadata.as_ref().req()?;
        // Package metadata is required in LF2 (LF1 had it gated behind
        // a feature flag); resolve name + version directly.
        let name =
            interned_strings.get(usize::try_from(metadata.name_interned_str).unwrap_or(usize::MAX)).req()?.to_owned();
        let version = interned_strings
            .get(usize::try_from(metadata.version_interned_str).unwrap_or(usize::MAX))
            .req()?
            .to_owned();
        let modules = package.modules.iter().map(DamlModulePayload::new).collect();
        Ok(Self {
            name,
            version: Some(version),
            language_version,
            package_id,
            interned_strings,
            interned_dotted_names,
            modules,
        })
    }
}
