use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;

use crate::convert::interned::PackageInternedResolver;
use crate::convert::module_payload::DamlModulePayload;
use crate::convert::util::Required;
use crate::error::{DamlLfConvertError, DamlLfConvertResult};
use crate::lf_protobuf::daml_lf_2;
use crate::{DamlLfArchive, DamlLfPackage, LanguageVersion};

/// Package-id → package-name lookup table shared across all
/// [`DamlPackagePayload`]s in the same archive. Each package holds
/// an `Arc`-clone so cross-package references in `convert_tycon_id`
/// can resolve the target package's name without threading the
/// archive through every helper.
///
/// For single-package payloads (the `apply_dalf` path) the map
/// carries only the self-mapping; cross-package lookups fall back
/// to the empty string the same way they did before this table
/// existed.
pub type PackageNameTable = Arc<HashMap<String, String>>;

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
    // Only consumed by `convert_expr` under `--features full`.
    #[allow(dead_code)]
    interned_exprs: &'a [daml_lf_2::Expr],
    pub modules: Vec<DamlModulePayload<'a>>,
    /// Cross-package package-id → package-name table. Populated by
    /// [`DamlArchivePayload::try_from`] (multi-package case); the
    /// stand-alone [`Self::try_from`] (single-package case) seeds
    /// this with only the self-mapping. Cloning an `Arc` is cheap;
    /// 30-package DARs are typical and each clone is one atomic
    /// increment.
    pkg_names: PackageNameTable,
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

    /// Raw LF2 expression-interning table — referenced by
    /// `Expr::Sum::InternedExpr(i)`. Only populated in LF 2.dev;
    /// empty in 2.1. Consumed by `convert_expr` under
    /// `--features full`.
    #[allow(dead_code)]
    pub fn interned_exprs_raw(&self) -> &'a [daml_lf_2::Expr] {
        self.interned_exprs
    }

    /// Look up the package-name for a given package-id. Returns
    /// `None` if the id isn't in this archive's name table — which
    /// happens either when the reference points outside the loaded
    /// archive or when the payload was constructed standalone
    /// (single-package path).
    pub fn cross_pkg_name(&self, package_id: &str) -> Option<&str> {
        self.pkg_names.get(package_id).map(String::as_str)
    }

    /// Replace this package's cross-package name table. Called by
    /// [`DamlArchivePayload::try_from`] once it has built the
    /// combined table from every package in the archive.
    pub(crate) fn set_pkg_names(&mut self, names: PackageNameTable) {
        self.pkg_names = names;
    }
}

impl PackageInternedResolver for DamlPackagePayload<'_> {
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
        let interned_exprs = package.interned_exprs.as_slice();
        let metadata = package.metadata.as_ref().req()?;
        // Package metadata is required in LF2 (LF1 had it gated behind
        // a feature flag); resolve name + version directly.
        let name = interned_strings
            .get(usize::try_from(metadata.name_interned_str).unwrap_or(usize::MAX))
            .ok_or_else(|| {
                DamlLfConvertError::InternalError(format!(
                    "package metadata name_interned_str {} out of range",
                    metadata.name_interned_str
                ))
            })?
            .to_owned();
        let version = interned_strings
            .get(usize::try_from(metadata.version_interned_str).unwrap_or(usize::MAX))
            .ok_or_else(|| {
                DamlLfConvertError::InternalError(format!(
                    "package metadata version_interned_str {} out of range",
                    metadata.version_interned_str
                ))
            })?
            .to_owned();
        let modules = package.modules.iter().map(DamlModulePayload::new).collect();
        // Seed the name table with the self-mapping. The archive
        // payload (multi-package case) replaces this with the full
        // table once it's collected every package's name.
        let mut pkg_names = HashMap::new();
        pkg_names.insert(package_id.to_owned(), name.clone());
        let pkg_names = Arc::new(pkg_names);
        Ok(Self {
            name,
            version: Some(version),
            language_version,
            package_id,
            interned_strings,
            interned_dotted_names,
            interned_types,
            interned_kinds,
            interned_exprs,
            modules,
            pkg_names,
        })
    }
}
