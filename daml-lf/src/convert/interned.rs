use std::convert::TryFrom;

use crate::convert::util::Required;
use crate::error::DamlLfConvertResult;

/// Resolves indices into a package's interning tables.
///
/// LF2 simplified the wire format by interning all strings and
/// dotted-names unconditionally — the LF1 `oneof literal | interned`
/// shapes are gone. The trait correspondingly has no
/// language-version gating or `literal` fall-back paths.
pub trait PackageInternedResolver {
    fn package_id(&self) -> &str;
    fn interned_strings(&self) -> &[String];
    /// Each entry is the slice of interned-string indices that
    /// compose one dotted-name. Borrowing the slice avoids cloning
    /// the proto representation.
    fn interned_dotted_names(&self) -> &[&[i32]];

    /// Resolve a single interned-string index.
    fn resolve_string(&self, index: i32) -> DamlLfConvertResult<&str> {
        let idx = usize::try_from(index).map_err(|_| crate::error::DamlLfConvertError::MissingRequiredField)?;
        Ok(self.interned_strings().get(idx).req()?.as_str())
    }

    /// Resolve a sequence of interned-string indices.
    fn resolve_strings(&self, indices: &[i32]) -> DamlLfConvertResult<Vec<&str>> {
        indices.iter().map(|&i| self.resolve_string(i)).collect()
    }

    /// Look up an interned-dotted-name's underlying interned-string
    /// indices (without resolving them to `&str` yet).
    fn resolve_dotted_to_indices(&self, index: i32) -> DamlLfConvertResult<&[i32]> {
        let idx = usize::try_from(index).map_err(|_| crate::error::DamlLfConvertError::MissingRequiredField)?;
        self.interned_dotted_names().get(idx).copied().req()
    }

    /// Fully resolve an interned-dotted-name to its constituent
    /// `&str` segments (e.g. `["Foo", "Bar", "Baz"]`).
    fn resolve_dotted(&self, index: i32) -> DamlLfConvertResult<Vec<&str>> {
        self.resolve_strings(self.resolve_dotted_to_indices(index)?)
    }
}
