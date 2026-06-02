use std::collections::HashMap;

use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::{ObjectMeta, PartyDetails};

/// The participant-side view of a Daml party.
///
/// v2 dropped `display_name` from the wire shape (`reserved 2` in the
/// proto) and added participant-local metadata plus an identity-provider
/// id. Modifiable fields can be updated via
/// `PartyManagementService.UpdatePartyDetails`.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlPartyDetails {
    /// Stable unique identifier of the party. Not modifiable.
    pub party: String,
    /// `true` iff the party is hosted by *this* participant and shares
    /// the same identity provider as the user issuing the request.
    /// Not modifiable.
    pub is_local: bool,
    /// Participant-local annotations + concurrency token. Modifiable.
    pub local_metadata: Option<DamlObjectMeta>,
    /// `Identity Provider` this party is assigned to. Empty string means
    /// the default IDP. Updatable via UpdatePartyIdentityProviderId, not
    /// UpdatePartyDetails.
    pub identity_provider_id: String,
}

impl From<PartyDetails> for DamlPartyDetails {
    fn from(d: PartyDetails) -> Self {
        Self {
            party: d.party,
            is_local: d.is_local,
            local_metadata: d.local_metadata.map(DamlObjectMeta::from),
            identity_provider_id: d.identity_provider_id,
        }
    }
}

impl From<DamlPartyDetails> for PartyDetails {
    fn from(d: DamlPartyDetails) -> Self {
        Self {
            party: d.party,
            is_local: d.is_local,
            local_metadata: d.local_metadata.map(Into::into),
            identity_provider_id: d.identity_provider_id,
        }
    }
}

/// Kubernetes-style metadata attached to participant-local resources
/// (parties, users, …).
///
/// `resource_version` is an opaque server-managed concurrency token: on
/// reads it carries the current version; on updates you echo it back to
/// have the server reject the update if anyone else has modified the
/// resource since you last read it. Leave it empty when creating a new
/// resource — the server populates it on success.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlObjectMeta {
    pub resource_version: String,
    pub annotations: HashMap<String, String>,
}

impl From<ObjectMeta> for DamlObjectMeta {
    fn from(m: ObjectMeta) -> Self {
        Self {
            resource_version: m.resource_version,
            annotations: m.annotations,
        }
    }
}

impl From<DamlObjectMeta> for ObjectMeta {
    fn from(m: DamlObjectMeta) -> Self {
        Self {
            resource_version: m.resource_version,
            annotations: m.annotations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_meta() -> DamlObjectMeta {
        DamlObjectMeta {
            resource_version: "v42".to_owned(),
            annotations: [("k".to_owned(), "v".to_owned()), ("other".to_owned(), "value".to_owned())]
                .into_iter()
                .collect(),
        }
    }

    fn sample_party() -> DamlPartyDetails {
        DamlPartyDetails {
            party: "Alice::participant".to_owned(),
            is_local: true,
            local_metadata: Some(sample_meta()),
            identity_provider_id: "idp-1".to_owned(),
        }
    }

    #[test]
    fn party_details_roundtrip() {
        let dto = sample_party();
        let proto: PartyDetails = dto.clone().into();
        let back: DamlPartyDetails = proto.into();
        assert_eq!(back, dto);
    }

    #[test]
    fn party_details_default_roundtrip() {
        let dto = DamlPartyDetails::default();
        let proto: PartyDetails = dto.clone().into();
        let back: DamlPartyDetails = proto.into();
        assert_eq!(back, dto);
    }

    #[test]
    fn party_details_without_metadata_roundtrip() {
        let dto = DamlPartyDetails {
            party: "Bob".to_owned(),
            is_local: false,
            local_metadata: None,
            identity_provider_id: String::new(),
        };
        let proto: PartyDetails = dto.clone().into();
        let back: DamlPartyDetails = proto.into();
        assert_eq!(back, dto);
    }

    #[test]
    fn object_meta_roundtrip() {
        let dto = sample_meta();
        let proto: ObjectMeta = dto.clone().into();
        let back: DamlObjectMeta = proto.into();
        assert_eq!(back, dto);
    }

    #[test]
    fn object_meta_empty_roundtrip() {
        let dto = DamlObjectMeta::default();
        let proto: ObjectMeta = dto.clone().into();
        let back: DamlObjectMeta = proto.into();
        assert_eq!(back, dto);
    }
}
