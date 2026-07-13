use std::convert::TryFrom;

use crate::data::party::DamlObjectMeta;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::{Right, User, right};
use crate::util::Required;

/// A participant user — the unit of authorization on the v2 Ledger API.
///
/// Users hold a set of [`DamlUserRight`]s that determine which parties
/// they may act/read/execute as, plus administrative scope. The `id`
/// is the primary key (and is what JWT subject claims address).
///
/// `metadata.resource_version` tracks changes to *this* message's fields
/// only. Granting or revoking rights does not bump the resource version.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlUser {
    /// User identifier. Required. Modifiable: no.
    pub id: String,
    /// The party this user reads and acts as by default (when they have
    /// the corresponding `CanActAs` / `CanReadAs` right). Empty for
    /// users that don't map to a single Daml party (e.g. participant
    /// admins).
    pub primary_party: String,
    /// When `true`, the user is locked out of the Ledger API entirely
    /// regardless of granted rights.
    pub is_deactivated: bool,
    /// Participant-local metadata (annotations + concurrency token).
    pub metadata: Option<DamlObjectMeta>,
    /// The `Identity Provider` this user is assigned to. Empty for the
    /// default IDP.
    pub identity_provider_id: String,
    /// When `true`, the user may authenticate via a Party JWT signed by
    /// `primary_party`'s signing key.
    pub primary_party_authentication: bool,
}

impl From<User> for DamlUser {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            primary_party: u.primary_party,
            is_deactivated: u.is_deactivated,
            metadata: u.metadata.map(DamlObjectMeta::from),
            identity_provider_id: u.identity_provider_id,
            primary_party_authentication: u.primary_party_authentication,
        }
    }
}

impl From<DamlUser> for User {
    fn from(u: DamlUser) -> Self {
        Self {
            id: u.id,
            primary_party: u.primary_party,
            is_deactivated: u.is_deactivated,
            metadata: u.metadata.map(Into::into),
            identity_provider_id: u.identity_provider_id,
            primary_party_authentication: u.primary_party_authentication,
        }
    }
}

/// A single right that can be granted to a [`DamlUser`].
///
/// `CanActAs(p)` implicitly includes `CanExecuteAs(p)`, but not
/// `CanReadAs(p)` — the wire protocol keeps the three orthogonal so
/// reads must be granted explicitly. The `AnyParty` variants are
/// participant-scoped (i.e. across every party hosted there) and are
/// typically held only by indexing tools (PQS, etc.) or multi-party
/// orchestrators.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum DamlUserRight {
    ParticipantAdmin,
    CanActAs(String),
    CanReadAs(String),
    CanExecuteAs(String),
    IdentityProviderAdmin,
    CanReadAsAnyParty,
    CanExecuteAsAnyParty,
}

impl From<DamlUserRight> for Right {
    fn from(r: DamlUserRight) -> Self {
        let kind = match r {
            DamlUserRight::ParticipantAdmin => right::Kind::ParticipantAdmin(right::ParticipantAdmin {}),
            DamlUserRight::CanActAs(party) => right::Kind::CanActAs(right::CanActAs {
                party,
            }),
            DamlUserRight::CanReadAs(party) => right::Kind::CanReadAs(right::CanReadAs {
                party,
            }),
            DamlUserRight::CanExecuteAs(party) => right::Kind::CanExecuteAs(right::CanExecuteAs {
                party,
            }),
            DamlUserRight::IdentityProviderAdmin => right::Kind::IdentityProviderAdmin(right::IdentityProviderAdmin {}),
            DamlUserRight::CanReadAsAnyParty => right::Kind::CanReadAsAnyParty(right::CanReadAsAnyParty {}),
            DamlUserRight::CanExecuteAsAnyParty => right::Kind::CanExecuteAsAnyParty(right::CanExecuteAsAnyParty {}),
        };
        Self {
            kind: Some(kind),
        }
    }
}

impl TryFrom<Right> for DamlUserRight {
    type Error = DamlError;

    fn try_from(r: Right) -> DamlResult<Self> {
        // The proto marks `kind` as required; an absent kind is a wire
        // violation. Surface it as an error rather than picking a
        // default — defaulting to e.g. `ParticipantAdmin` would silently
        // elevate privileges, and we don't have a benign "unknown"
        // variant that callers could safely ignore.
        match r.kind.req()? {
            right::Kind::ParticipantAdmin(_) => Ok(Self::ParticipantAdmin),
            right::Kind::CanActAs(r) => Ok(Self::CanActAs(r.party)),
            right::Kind::CanReadAs(r) => Ok(Self::CanReadAs(r.party)),
            right::Kind::CanExecuteAs(r) => Ok(Self::CanExecuteAs(r.party)),
            right::Kind::IdentityProviderAdmin(_) => Ok(Self::IdentityProviderAdmin),
            right::Kind::CanReadAsAnyParty(_) => Ok(Self::CanReadAsAnyParty),
            right::Kind::CanExecuteAsAnyParty(_) => Ok(Self::CanExecuteAsAnyParty),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_user() -> DamlUser {
        DamlUser {
            id: "user-x".to_owned(),
            primary_party: "Alice::participant".to_owned(),
            is_deactivated: true,
            metadata: Some(DamlObjectMeta {
                resource_version: "v7".to_owned(),
                annotations: std::iter::once(("k".to_owned(), "v".to_owned())).collect(),
            }),
            identity_provider_id: "idp-1".to_owned(),
            primary_party_authentication: true,
        }
    }

    #[test]
    fn user_roundtrip() {
        let dto = sample_user();
        let proto: User = dto.clone().into();
        let back: DamlUser = proto.into();
        assert_eq!(back, dto);
    }

    #[test]
    fn user_default_roundtrip() {
        let dto = DamlUser::default();
        let proto: User = dto.clone().into();
        let back: DamlUser = proto.into();
        assert_eq!(back, dto);
    }

    /// Every `DamlUserRight` variant must round-trip cleanly: a v2
    /// proto with a missing kind would be a wire violation and is
    /// covered separately in [`right_with_missing_kind_errors`].
    /// Critically the participant-scoped (`AnyParty`) and
    /// IdP-scoped (`IdentityProviderAdmin`) variants must not get
    /// silently confused with `ParticipantAdmin` — the convert
    /// layer does *not* fall through to a default.
    #[test]
    fn every_right_variant_roundtrips() {
        let cases = [
            DamlUserRight::ParticipantAdmin,
            DamlUserRight::CanActAs("Alice".to_owned()),
            DamlUserRight::CanReadAs("Bob".to_owned()),
            DamlUserRight::CanExecuteAs("Charlie".to_owned()),
            DamlUserRight::IdentityProviderAdmin,
            DamlUserRight::CanReadAsAnyParty,
            DamlUserRight::CanExecuteAsAnyParty,
        ];
        for original in cases {
            let proto: Right = original.clone().into();
            let back = DamlUserRight::try_from(proto).expect("known kinds should round-trip");
            assert_eq!(back, original, "right variant did not round-trip");
        }
    }

    #[test]
    fn right_with_missing_kind_errors() {
        let proto = Right {
            kind: None,
        };
        let result = DamlUserRight::try_from(proto);
        assert!(matches!(result, Err(DamlError::MissingRequiredField)));
    }
}
