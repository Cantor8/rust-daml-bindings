use std::convert::TryFrom;

use crate::convert::interned::PackageInternedResolver;
use crate::convert::module_payload::DamlModulePayload;
use crate::convert::util::Required;
use crate::error::{DamlLfConvertError, DamlLfConvertResult};
use crate::lf_protobuf::daml_lf_2;
use crate::{DamlLfArchive, DamlLfPackage, LanguageVersion};

/// Borrowed view of an LF2 `Package`: metadata (name, version,
/// language version, package-id), the package-level interning
/// tables (strings, dotted-names, types, kinds), and the list of
/// [`DamlModulePayload`]s.
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
    interned_types: &'a [daml_lf_2::Type],
    interned_kinds: &'a [daml_lf_2::Kind],
    pub modules: Vec<DamlModulePayload<'a>>,
}

impl<'a> DamlPackagePayload<'a> {
    /// Raw LF2 type-interning table — referenced by
    /// `Type::Sum::InternedType(i)`. Always non-empty when the
    /// package opts into type interning (most packages do).
    pub fn interned_types_raw(&self) -> &'a [daml_lf_2::Type] {
        self.interned_types
    }

    /// Raw LF2 kind-interning table — referenced by
    /// `Kind::Sum::InternedKind(i)`. Only populated in LF 2.dev;
    /// empty in 2.1.
    pub fn interned_kinds_raw(&self) -> &'a [daml_lf_2::Kind] {
        self.interned_kinds
    }
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
        let interned_types = package.interned_types.as_slice();
        let interned_kinds = package.interned_kinds.as_slice();
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
            interned_types,
            interned_kinds,
            modules,
        })
    }
}
