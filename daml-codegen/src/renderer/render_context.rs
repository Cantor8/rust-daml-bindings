use daml_lf::element::DamlArchive;

/// Contextual information required during code generation.
#[derive(Debug, Default)]
pub struct RenderContext<'a> {
    mode: RenderMode<'a>,
    filter_mode: RenderFilterMode,
}

impl<'a> RenderContext<'a> {
    /// Create a new `RenderContext` with a `DamlArchive`.
    pub const fn with_archive(archive: &'a DamlArchive<'a>, filter_mode: RenderFilterMode) -> Self {
        Self {
            mode: RenderMode::Full(archive),
            filter_mode,
        }
    }

    pub const fn archive(&self) -> &DamlArchive<'_> {
        match &self.mode {
            RenderMode::Intermediate(archive) => archive,
            RenderMode::Full(archive) => archive,
        }
    }

    pub const fn filter_mode(&self) -> RenderFilterMode {
        self.filter_mode
    }

    /// Look up the package-name for the package with the given
    /// package-id, returning `None` when the archive doesn't contain
    /// such a package or the package has no name (older LF archives
    /// produced before LF2 made `PackageMetadata` mandatory).
    pub fn package_name_for(&self, package_id: &str) -> Option<&str> {
        let pkg = self.archive().packages().find(|p| p.package_id() == package_id)?;
        let name = pkg.name();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }
}

/// Rendering mode.
#[derive(Debug)]
pub enum RenderMode<'a> {
    Intermediate(DamlArchive<'a>),
    Full(&'a DamlArchive<'a>),
}

impl Default for RenderMode<'_> {
    fn default() -> Self {
        RenderMode::Intermediate(DamlArchive::default())
    }
}

/// Rendering filter mode.
#[derive(Debug, Clone, Copy, Default)]
pub enum RenderFilterMode {
    /// Exclude only fields with type constructors that contain Higher Kinded Types (HKT) only.
    #[default]
    HigherKindedType,
    /// Exclude all non-serializable fields.
    NonSerializable,
}
