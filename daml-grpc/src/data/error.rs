use std::io;
use tonic::codegen::http;

/// A Daml ledger error.
#[derive(Debug, thiserror::Error)]
pub enum DamlError {
    #[error("timeout error: {0}")]
    TimeoutError(#[source] Box<DamlError>),
    #[error(transparent)]
    GrpcTransportError(#[from] tonic::transport::Error),
    #[error(transparent)]
    GrpcStatusError(tonic::Status),
    #[error(transparent)]
    GrpcPermissionError(tonic::Status),
    #[error(transparent)]
    InvalidUriError(#[from] http::uri::InvalidUri),
    #[error(transparent)]
    StdError(#[from] io::Error),
    #[error("unexpected type, expected {0} but found {1}")]
    UnexpectedType(String, String),
    #[error("unknown field {0}")]
    UnknownField(String),
    #[error("list index {0} out of range")]
    ListIndexOutOfRange(usize),
    #[error("expected optional value is None")]
    MissingRequiredField,
    #[error("unexpected variant constructor, expected {0} but found {1}")]
    UnexpectedVariant(String, String),
    #[error("{0}")]
    Other(String),
    #[error("failed conversion: {0}")]
    FailedConversion(String),
    #[error("insufficient parties")]
    InsufficientParties,
}

impl DamlError {
    pub fn new_failed_conversion(msg: impl Into<String>) -> Self {
        DamlError::FailedConversion(msg.into())
    }

    pub fn new_timeout_error(inner: DamlError) -> Self {
        DamlError::TimeoutError(Box::new(inner))
    }
}

/// `tonic::Status` maps to one of two variants depending on its code —
/// permission-vs-authn errors get their own bucket so downstream code
/// can pattern-match on it without inspecting the status code.
impl From<tonic::Status> for DamlError {
    fn from(e: tonic::Status) -> Self {
        match e.code() {
            tonic::Code::PermissionDenied | tonic::Code::Unauthenticated => DamlError::GrpcPermissionError(e),
            _ => DamlError::GrpcStatusError(e),
        }
    }
}

impl From<&str> for DamlError {
    fn from(e: &str) -> Self {
        DamlError::Other(e.to_owned())
    }
}

impl From<bigdecimal::ParseBigDecimalError> for DamlError {
    fn from(e: bigdecimal::ParseBigDecimalError) -> Self {
        DamlError::FailedConversion(e.to_string())
    }
}

/// A Daml ledger result.
pub type DamlResult<T> = ::std::result::Result<T, DamlError>;
