use std::convert::TryFrom;

use bytes::Bytes;
use itertools::Itertools;
use prost::Message;

use crate::element::DamlPackage;
use crate::error::{DamlLfError, DamlLfResult};
use crate::lf_protobuf::daml_lf::archive_payload::Sum;
use crate::lf_protobuf::daml_lf::ArchivePayload;
use crate::lf_protobuf::daml_lf_2;
use crate::{convert, LanguageV2MinorVersion, LanguageVersion};

/// A `Daml LF` archive payload (aka "package"): a `language_version`
/// plus the decoded LF2 package AST.
#[derive(Debug, Clone)]
pub struct DamlLfArchivePayload {
    pub language_version: LanguageVersion,
    pub package: DamlLfPackage,
}

impl DamlLfArchivePayload {
    /// Construct from an existing language version + package.
    pub const fn new(language_version: LanguageVersion, package: DamlLfPackage) -> Self {
        Self {
            language_version,
            package,
        }
    }

    /// Decode a `DamlLfArchivePayload` from the raw bytes returned by
    /// `PackageService.GetPackage` (the `archive_payload` field of
    /// `GetPackageResponse`).
    ///
    /// The wire format is two layers: an outer `ArchivePayload`
    /// envelope carrying the minor-version string and a `bytes`
    /// payload, and an inner LF2 `Package` message inside those
    /// bytes. The double-encoding is deliberate — the LF authors keep
    /// it so the participant can apply a different protobuf
    /// recursion limit per minor version without recompiling clients.
    ///
    /// Returns `UnknownVersion` for archives whose envelope advertises
    /// a non-LF2 sum (LF1 is no longer supported by this crate; an
    /// LF1 archive will surface as an error here).
    pub fn from_bytes(payload_buffer: impl Into<Bytes>) -> DamlLfResult<Self> {
        let envelope: ArchivePayload = ArchivePayload::decode(payload_buffer.into())?;
        let minor = LanguageV2MinorVersion::try_from(envelope.minor.as_str())?;
        match envelope.sum {
            Some(Sum::DamlLf2(bytes)) => {
                let package = daml_lf_2::Package::decode(bytes.as_slice())?;
                Ok(Self::new(LanguageVersion::new_v2(minor), DamlLfPackage::V2(package)))
            },
            Some(Sum::DamlLf1(_)) => Err(DamlLfError::new_unknown_version("v1 (LF1 is no longer supported)")),
            None => Err(DamlLfError::new_unknown_version("none")),
        }
    }

    /// Create a [`DamlArchive`] from this [`DamlLfArchivePayload`] and apply `f` to it.
    ///
    /// [`DamlArchive`]: crate::element::DamlArchive
    pub fn apply<R, F>(self, f: F) -> DamlLfResult<R>
    where
        F: FnOnce(&DamlPackage<'_>) -> R,
    {
        convert::apply_payload(self, f)
    }

    /// Returns true if the embedded package contains a module named
    /// `module` (in dotted-name form, e.g. `Foo.Bar.Baz`).
    pub fn contains_module(&self, module: &str) -> bool {
        match &self.package {
            DamlLfPackage::V2(package) => package
                .modules
                .iter()
                .any(|m| Self::decode_dotted_name(package, m.name_interned_dname).as_deref() == Some(module)),
        }
    }

    /// Returns every module name in the embedded package, each in
    /// dotted-name form.
    pub fn list_modules(&self) -> Vec<String> {
        match &self.package {
            DamlLfPackage::V2(package) => package
                .modules
                .iter()
                .filter_map(|m| Self::decode_dotted_name(package, m.name_interned_dname))
                .collect(),
        }
    }

    pub const fn language_version(&self) -> &LanguageVersion {
        &self.language_version
    }

    pub const fn package(&self) -> &DamlLfPackage {
        &self.package
    }

    /// Resolve a dotted-name interning index into a `Foo.Bar.Baz`
    /// string by chasing through the package's interned-string and
    /// interned-dotted-name tables. Returns `None` if the indexes are
    /// out of bounds (a wire violation, but not worth panicking over).
    fn decode_dotted_name(package: &daml_lf_2::Package, idx: i32) -> Option<String> {
        let dotted = package.interned_dotted_names.get(usize::try_from(idx).ok()?)?;
        let parts: Option<Vec<&str>> = dotted
            .segments_interned_str
            .iter()
            .map(|&i| package.interned_strings.get(usize::try_from(i).ok()?).map(String::as_str))
            .collect();
        parts.map(|p| p.into_iter().join("."))
    }
}

/// The supported Daml-LF package formats. LF1 was removed in this
/// crate's v2; the enum is kept so a future LF3 has somewhere to
/// land.
#[derive(Debug, Clone)]
pub enum DamlLfPackage {
    V2(daml_lf_2::Package),
}
