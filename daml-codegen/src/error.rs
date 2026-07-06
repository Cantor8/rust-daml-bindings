use daml_lf::DamlLfError;

/// Daml code generator errors.
#[derive(Debug, thiserror::Error)]
pub enum DamlCodeGenError {
    /// An invalid module matcher regex was provided.
    #[error("invalid module matcher regex: {0}")]
    InvalidModuleMatcherRegex(#[from] regex::Error),
    /// Daml LF error.
    #[error("Daml LF error: {0}")]
    DamlLfError(#[from] DamlLfError),
    /// IO error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Daml code generator result.
pub type DamlCodeGenResult<T> = Result<T, DamlCodeGenError>;
