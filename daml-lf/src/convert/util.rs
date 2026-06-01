use crate::error::{DamlLfConvertError, DamlLfConvertResult};

/// Extract the value from an `Option`, or surface a wire-violation
/// error. prost models every proto message field as `Option<T>`,
/// even those marked `required`; this trait turns the resulting
/// `Option` into a `Result` we can `?`-propagate.
pub trait Required<T> {
    fn req(self) -> DamlLfConvertResult<T>;
}

impl<T> Required<T> for Option<T> {
    fn req(self) -> DamlLfConvertResult<T> {
        self.ok_or(DamlLfConvertError::MissingRequiredField)
    }
}
