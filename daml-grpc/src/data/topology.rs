use std::convert::TryFrom;

use chrono::{DateTime, Utc};

use crate::data::offset::DamlLedgerOffset;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::topology_event::Event as TopologyEventKind;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{
    ParticipantAuthorizationAdded, ParticipantAuthorizationChanged, ParticipantAuthorizationOnboarding,
    ParticipantAuthorizationRevoked, ParticipantPermission, TopologyEvent, TopologyTransaction,
};
use crate::util;
use crate::util::Required;

/// The scope of authority a participant has been granted over a party
/// on a given synchronizer.
///
/// The proto's `Unspecified` value exists for wire-default reasons and
/// is documented as not intended for use; surfaced here so we can
/// faithfully round-trip whatever the server sends but should not be
/// used in code.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum DamlParticipantPermission {
    #[default]
    Unspecified,
    /// The participant can submit commands on behalf of the party.
    Submission,
    /// The participant can confirm transactions on behalf of the
    /// party but cannot initiate them.
    Confirmation,
    /// The participant can observe transactions touching the party
    /// but cannot confirm or submit.
    Observation,
}

impl From<ParticipantPermission> for DamlParticipantPermission {
    fn from(p: ParticipantPermission) -> Self {
        match p {
            ParticipantPermission::Unspecified => Self::Unspecified,
            ParticipantPermission::Submission => Self::Submission,
            ParticipantPermission::Confirmation => Self::Confirmation,
            ParticipantPermission::Observation => Self::Observation,
        }
    }
}

/// A topology change observed on a synchronizer — typically the
/// addition, modification, or removal of a (party, participant,
/// permission) authorization.
///
/// v2 emits topology transactions on the update stream alongside
/// regular `Transaction`s and `Reassignment`s. Subscribe by setting
/// `DamlUpdateFormat::include_topology_events`.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlTopologyTransaction {
    pub update_id: String,
    pub offset: DamlLedgerOffset,
    pub synchronizer_id: String,
    /// Effective time of the topology change (which may lag the
    /// sequencing time slightly). Topology transactions are ordered
    /// per-synchronizer by effective time, not by sequencing time.
    pub record_time: DateTime<Utc>,
    pub events: Vec<DamlTopologyEvent>,
}

impl TryFrom<TopologyTransaction> for DamlTopologyTransaction {
    type Error = DamlError;

    fn try_from(t: TopologyTransaction) -> DamlResult<Self> {
        Ok(Self {
            update_id: t.update_id,
            offset: DamlLedgerOffset::new(t.offset),
            synchronizer_id: t.synchronizer_id,
            record_time: util::from_grpc_timestamp(&t.record_time.req()?),
            events: t.events.into_iter().map(DamlTopologyEvent::try_from).collect::<DamlResult<_>>()?,
        })
    }
}

/// One step in a topology transaction. Three of the four variants
/// share an `(party_id, participant_id, permission)` shape; `Revoked`
/// drops the permission (the authorization is being removed).
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlTopologyEvent {
    ParticipantAuthorizationAdded(DamlParticipantAuthorizationAdded),
    ParticipantAuthorizationChanged(DamlParticipantAuthorizationChanged),
    ParticipantAuthorizationRevoked(DamlParticipantAuthorizationRevoked),
    ParticipantAuthorizationOnboarding(DamlParticipantAuthorizationOnboarding),
}

impl TryFrom<TopologyEvent> for DamlTopologyEvent {
    type Error = DamlError;

    fn try_from(e: TopologyEvent) -> DamlResult<Self> {
        Ok(match e.event.req()? {
            TopologyEventKind::ParticipantAuthorizationAdded(e) =>
                Self::ParticipantAuthorizationAdded(DamlParticipantAuthorizationAdded::try_from(e)?),
            TopologyEventKind::ParticipantAuthorizationChanged(e) =>
                Self::ParticipantAuthorizationChanged(DamlParticipantAuthorizationChanged::try_from(e)?),
            TopologyEventKind::ParticipantAuthorizationRevoked(e) =>
                Self::ParticipantAuthorizationRevoked(DamlParticipantAuthorizationRevoked::from(e)),
            TopologyEventKind::ParticipantAuthorizationOnboarding(e) =>
                Self::ParticipantAuthorizationOnboarding(DamlParticipantAuthorizationOnboarding::try_from(e)?),
        })
    }
}

/// A new (party, participant, permission) authorization was added.
/// Together with `Onboarding`, this is how a party joins a
/// participant.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlParticipantAuthorizationAdded {
    pub party_id: String,
    pub participant_id: String,
    pub permission: DamlParticipantPermission,
}

impl TryFrom<ParticipantAuthorizationAdded> for DamlParticipantAuthorizationAdded {
    type Error = DamlError;

    fn try_from(e: ParticipantAuthorizationAdded) -> DamlResult<Self> {
        Ok(Self {
            party_id: e.party_id,
            participant_id: e.participant_id,
            permission: DamlParticipantPermission::from(ParticipantPermission::from_i32(e.participant_permission).req()?),
        })
    }
}

/// The permission on an existing (party, participant) authorization
/// was modified.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlParticipantAuthorizationChanged {
    pub party_id: String,
    pub participant_id: String,
    pub permission: DamlParticipantPermission,
}

impl TryFrom<ParticipantAuthorizationChanged> for DamlParticipantAuthorizationChanged {
    type Error = DamlError;

    fn try_from(e: ParticipantAuthorizationChanged) -> DamlResult<Self> {
        Ok(Self {
            party_id: e.party_id,
            participant_id: e.participant_id,
            permission: DamlParticipantPermission::from(ParticipantPermission::from_i32(e.participant_permission).req()?),
        })
    }
}

/// An existing authorization was revoked. The party is no longer
/// associated with the participant on this synchronizer.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlParticipantAuthorizationRevoked {
    pub party_id: String,
    pub participant_id: String,
}

impl From<ParticipantAuthorizationRevoked> for DamlParticipantAuthorizationRevoked {
    fn from(e: ParticipantAuthorizationRevoked) -> Self {
        Self {
            party_id: e.party_id,
            participant_id: e.participant_id,
        }
    }
}

/// A party was onboarded — a special form of `Added` that signals an
/// initial association rather than a permission change.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlParticipantAuthorizationOnboarding {
    pub party_id: String,
    pub participant_id: String,
    pub permission: DamlParticipantPermission,
}

impl TryFrom<ParticipantAuthorizationOnboarding> for DamlParticipantAuthorizationOnboarding {
    type Error = DamlError;

    fn try_from(e: ParticipantAuthorizationOnboarding) -> DamlResult<Self> {
        Ok(Self {
            party_id: e.party_id,
            participant_id: e.participant_id,
            permission: DamlParticipantPermission::from(ParticipantPermission::from_i32(e.participant_permission).req()?),
        })
    }
}
