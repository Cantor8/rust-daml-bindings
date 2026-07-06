use std::convert::TryFrom;

use crate::data::event::DamlCreatedEvent;
use crate::data::offset::DamlLedgerOffset;
use crate::data::reassignment::{DamlAssignedEvent, DamlUnassignedEvent};
use crate::data::topology::DamlParticipantPermission;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::get_active_contracts_response::ContractEntry;
use crate::grpc_protobuf::com::daml::ledger::api::v2::get_connected_synchronizers_response::ConnectedSynchronizer;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{
    ActiveContract, GetActiveContractsResponse, IncompleteAssigned, IncompleteUnassigned, ParticipantPermission,
};
use crate::util::Required;

/// A snapshot of one (contract, synchronizer) pair at a specific
/// ledger offset.
///
/// Activeness is a per-synchronizer concept: a contract can be active
/// on one synchronizer while archived on another. The same contract
/// can therefore appear multiple times in a snapshot, once per
/// synchronizer it is active on.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlActiveContract {
    /// The most recent create-or-assign event for this contract on
    /// `synchronizer_id`. The event's offset may point at an already-
    /// pruned update; do not assume it's lookup-able.
    pub created_event: DamlCreatedEvent,
    pub synchronizer_id: String,
    /// `0` when the contract has never been reassigned; strictly
    /// increases with each unassign.
    pub reassignment_counter: u64,
}

impl TryFrom<ActiveContract> for DamlActiveContract {
    type Error = DamlError;

    fn try_from(c: ActiveContract) -> DamlResult<Self> {
        Ok(Self {
            created_event: DamlCreatedEvent::try_from(c.created_event.req()?)?,
            synchronizer_id: c.synchronizer_id,
            reassignment_counter: c.reassignment_counter,
        })
    }
}

/// A contract that was unassigned at or before the snapshot offset
/// but whose matching `Assigned` hasn't been observed yet.
///
/// The contract is in an in-between state: visible on neither the
/// source synchronizer (because it was unassigned) nor the target
/// (because the assign hasn't landed). The `CreatedEvent` represents
/// its prior state on the source.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlIncompleteUnassigned {
    pub created_event: DamlCreatedEvent,
    pub unassigned_event: DamlUnassignedEvent,
}

impl TryFrom<IncompleteUnassigned> for DamlIncompleteUnassigned {
    type Error = DamlError;

    fn try_from(u: IncompleteUnassigned) -> DamlResult<Self> {
        Ok(Self {
            created_event: DamlCreatedEvent::try_from(u.created_event.req()?)?,
            unassigned_event: DamlUnassignedEvent::try_from(u.unassigned_event.req()?)?,
        })
    }
}

/// A contract that was assigned at or before the snapshot offset but
/// whose matching `Unassigned` hasn't been observed yet.
///
/// Note: per the proto, this **does not** mean the contract is active
/// on the target — only that the participant has seen the assign half
/// without the matching unassign half yet.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlIncompleteAssigned {
    pub assigned_event: DamlAssignedEvent,
}

impl TryFrom<IncompleteAssigned> for DamlIncompleteAssigned {
    type Error = DamlError;

    fn try_from(a: IncompleteAssigned) -> DamlResult<Self> {
        Ok(Self {
            assigned_event: DamlAssignedEvent::try_from(a.assigned_event.req()?)?,
        })
    }
}

/// One entry in the active-contracts snapshot — either a regular
/// active contract or one of the two "incomplete" reassignment
/// states. The variant tells you which.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlContractEntry {
    Active(DamlActiveContract),
    IncompleteUnassigned(Box<DamlIncompleteUnassigned>),
    IncompleteAssigned(Box<DamlIncompleteAssigned>),
}

impl TryFrom<ContractEntry> for DamlContractEntry {
    type Error = DamlError;

    fn try_from(e: ContractEntry) -> DamlResult<Self> {
        Ok(match e {
            ContractEntry::ActiveContract(c) => Self::Active(DamlActiveContract::try_from(c)?),
            ContractEntry::IncompleteUnassigned(u) =>
                Self::IncompleteUnassigned(Box::new(DamlIncompleteUnassigned::try_from(u)?)),
            ContractEntry::IncompleteAssigned(a) =>
                Self::IncompleteAssigned(Box::new(DamlIncompleteAssigned::try_from(a)?)),
        })
    }
}

/// One message in the `GetActiveContracts` stream — carries one
/// contract entry plus, on the streaming RPC only, the continuation
/// token to resume from after this message.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlActiveContractsResponse {
    pub workflow_id: String,
    pub contract_entry: DamlContractEntry,
    /// Opaque resume-cursor. Empty when not applicable (point-fetched
    /// pages use a separate `next_page_token`).
    pub stream_continuation_token: Vec<u8>,
}

impl TryFrom<GetActiveContractsResponse> for DamlActiveContractsResponse {
    type Error = DamlError;

    fn try_from(r: GetActiveContractsResponse) -> DamlResult<Self> {
        Ok(Self {
            workflow_id: r.workflow_id,
            contract_entry: DamlContractEntry::try_from(r.contract_entry.req()?)?,
            stream_continuation_token: r.stream_continuation_token,
        })
    }
}

/// A single page of `GetActiveContractsPage` results. The
/// `active_at_offset` echoes the request (or the participant's
/// chosen current offset when the request didn't specify one).
/// `next_page_token = None` marks the last page.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlActiveContractsPage {
    pub active_contracts: Vec<DamlActiveContractsResponse>,
    pub active_at_offset: DamlLedgerOffset,
    pub next_page_token: Option<Vec<u8>>,
}

/// One synchronizer the participant is connected to. The optional
/// `permission` is only populated when the query was scoped to a
/// specific party.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlConnectedSynchronizer {
    pub synchronizer_alias: String,
    pub synchronizer_id: String,
    pub permission: Option<DamlParticipantPermission>,
}

impl TryFrom<ConnectedSynchronizer> for DamlConnectedSynchronizer {
    type Error = DamlError;

    fn try_from(c: ConnectedSynchronizer) -> DamlResult<Self> {
        // `permission` is wire-default `Unspecified` when "not set"
        // (proto doesn't model it as a separate Option). Treat
        // Unspecified as None so callers can distinguish "no party
        // scoped — no permission to report" from a real permission.
        let permission = match ParticipantPermission::try_from(c.permission).ok().req()? {
            ParticipantPermission::Unspecified => None,
            other => Some(DamlParticipantPermission::from(other)),
        };
        Ok(Self {
            synchronizer_alias: c.synchronizer_alias,
            synchronizer_id: c.synchronizer_id,
            permission,
        })
    }
}

/// The participant's currently-known prune offsets. Both fields are
/// `0` when nothing has been pruned yet on that axis.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct DamlLatestPrunedOffsets {
    /// Offset up to which the participant's main store has been
    /// pruned (inclusive). Does not factor in divulged-contracts
    /// pruning.
    pub participant_pruned_up_to_inclusive: DamlLedgerOffset,
    /// Offset up to which divulged events have been pruned.
    /// Always at or before `participant_pruned_up_to_inclusive`.
    pub all_divulged_contracts_pruned_up_to_inclusive: DamlLedgerOffset,
}
