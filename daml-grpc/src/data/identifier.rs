use crate::grpc_protobuf::com::daml::ledger::api::v2::Identifier;
use std::fmt::{Display, Formatter, Result};

/// Sentinel prefix used by the v2 Ledger API to discriminate package-name
/// references from package-id references in the `package_id` field of
/// [`Identifier`]. `#` is not a valid character in a package-id, so its
/// presence as the first character unambiguously marks a package-name.
const PACKAGE_NAME_PREFIX: char = '#';

/// Unique identifier of an entity on a Daml ledger.
///
/// In the v2 Ledger API the `package_id` field of the underlying protobuf
/// `Identifier` is overloaded: it carries either a package-id or — for
/// templates and interfaces — a package-name encoded as `#<package-name>`.
/// This type preserves that representation as-is on the wire; the
/// [`DamlIdentifier::package_name`] accessor extracts the bare name when the
/// reference is a package-name, and [`DamlIdentifier::package_id`] returns
/// the package-id when it isn't.
#[derive(Debug, PartialEq, Eq, Default, Clone, Hash, Ord, PartialOrd)]
pub struct DamlIdentifier {
    package_ref: String,
    module_name: String,
    entity_name: String,
}

impl DamlIdentifier {
    /// Construct an identifier from a raw package reference (either a
    /// package-id, or a package-name already prefixed with `#`).
    pub fn new(
        package_ref: impl Into<String>,
        module_name: impl Into<String>,
        entity_name: impl Into<String>,
    ) -> Self {
        Self {
            package_ref: package_ref.into(),
            module_name: module_name.into(),
            entity_name: entity_name.into(),
        }
    }

    /// Construct an identifier addressed by package-name. The on-wire form
    /// is `#<package_name>`.
    pub fn from_package_name(
        package_name: impl AsRef<str>,
        module_name: impl Into<String>,
        entity_name: impl Into<String>,
    ) -> Self {
        Self::new(
            format!("{PACKAGE_NAME_PREFIX}{}", package_name.as_ref()),
            module_name,
            entity_name,
        )
    }

    /// The raw package reference exactly as it appears on the wire — either
    /// a package-id or a `#`-prefixed package-name. Prefer
    /// [`Self::package_id`] / [`Self::package_name`] when the variant
    /// matters.
    pub fn package_ref(&self) -> &str {
        &self.package_ref
    }

    /// Returns the package-id when this identifier addresses a package by
    /// id, or `None` if it carries a package-name reference instead.
    pub fn package_id(&self) -> Option<&str> {
        if self.is_package_name() {
            None
        } else {
            Some(&self.package_ref)
        }
    }

    /// Returns the package-name when this identifier addresses a template
    /// or interface by package-name, or `None` if it carries a package-id.
    pub fn package_name(&self) -> Option<&str> {
        self.package_ref.strip_prefix(PACKAGE_NAME_PREFIX)
    }

    /// `true` when this identifier addresses a package by name.
    pub fn is_package_name(&self) -> bool {
        self.package_ref.starts_with(PACKAGE_NAME_PREFIX)
    }

    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    pub fn entity_name(&self) -> &str {
        &self.entity_name
    }
}

impl Display for DamlIdentifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}:{}:{}", self.package_ref, self.module_name, self.entity_name)
    }
}

impl From<DamlIdentifier> for Identifier {
    fn from(id: DamlIdentifier) -> Self {
        Self {
            package_id: id.package_ref,
            module_name: id.module_name,
            entity_name: id.entity_name,
        }
    }
}

impl From<Identifier> for DamlIdentifier {
    fn from(id: Identifier) -> Self {
        Self::new(id.package_id, id.module_name, id.entity_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_id_reference_roundtrips() {
        let id = DamlIdentifier::new("abc123", "Foo.Bar", "Baz");
        assert_eq!(id.package_id(), Some("abc123"));
        assert_eq!(id.package_name(), None);
        assert!(!id.is_package_name());
    }

    #[test]
    fn package_name_reference_roundtrips() {
        let id = DamlIdentifier::from_package_name("my-pkg", "Foo.Bar", "Baz");
        assert_eq!(id.package_ref(), "#my-pkg");
        assert_eq!(id.package_name(), Some("my-pkg"));
        assert_eq!(id.package_id(), None);
        assert!(id.is_package_name());
    }

    #[test]
    fn display_uses_wire_form() {
        let by_id = DamlIdentifier::new("abc", "M", "E");
        assert_eq!(by_id.to_string(), "abc:M:E");
        let by_name = DamlIdentifier::from_package_name("pkg", "M", "E");
        assert_eq!(by_name.to_string(), "#pkg:M:E");
    }

    #[test]
    fn proto_conversion_is_lossless() {
        let original = DamlIdentifier::from_package_name("pkg", "M", "E");
        let proto: Identifier = original.clone().into();
        assert_eq!(proto.package_id, "#pkg");
        let round_tripped = DamlIdentifier::from(proto);
        assert_eq!(round_tripped, original);
    }
}
