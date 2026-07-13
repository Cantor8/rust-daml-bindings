use std::convert::TryFrom;

use chrono::{DateTime, Utc};

use crate::data::event::DamlCreatedEvent;
use crate::data::identifier::DamlIdentifier;
use crate::data::offset::DamlLedgerOffset;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::reassignment_command::Command as ReassignmentCommandKind;
use crate::grpc_protobuf::com::daml::ledger::api::v2::reassignment_event::Event as ReassignmentEventKind;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{
    AssignCommand, AssignedEvent, Reassignment, ReassignmentCommand, ReassignmentCommands, ReassignmentEvent,
    UnassignCommand, UnassignedEvent,
};
use crate::util;
use crate::util::Required;

// ---------------------------------------------------------------------------
// Response-side: Reassignment + ReassignmentEvent + Unassigned/Assigned
// ---------------------------------------------------------------------------

/// An on-ledger reassignment, which moves a contract across
/// synchronizers in two steps (unassign on the source, assign on the
/// target). The protocol emits a separate `DamlReassignment` for each
/// of the two halves: the source-synchronizer reassignment contains an
/// `Unassigned` event, the target-synchronizer one an `Assigned` event.
///
/// `command_id` is empty for everyone except the submitting party on
/// the submitting participant; `workflow_id` is empty when the
/// originating command didn't set one.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlReassignment {
    pub update_id: String,
    pub command_id: String,
    pub workflow_id: String,
    pub offset: DamlLedgerOffset,
    pub events: Vec<DamlReassignmentEvent>,
    /// Record time on the synchronizer this `Reassignment` came from
    /// — the source for unassign events, the target for assign events.
    pub record_time: DateTime<Utc>,
    pub synchronizer_id: String,
    pub paid_traffic_cost: Option<i64>,
}

impl TryFrom<Reassignment> for DamlReassignment {
    type Error = DamlError;

    fn try_from(r: Reassignment) -> DamlResult<Self> {
        Ok(Self {
            update_id: r.update_id,
            command_id: r.command_id,
            workflow_id: r.workflow_id,
            offset: DamlLedgerOffset::new(r.offset),
            events: r.events.into_iter().map(DamlReassignmentEvent::try_from).collect::<DamlResult<_>>()?,
            record_time: util::from_grpc_timestamp(&r.record_time.req()?)?,
            synchronizer_id: r.synchronizer_id,
            paid_traffic_cost: r.paid_traffic_cost,
        })
    }
}

/// One event in a [`DamlReassignment`]: either the source-side
/// `Unassigned` half or the target-side `Assigned` half. The same
/// `reassignment_counter` ties matching halves together.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlReassignmentEvent {
    Unassigned(Box<DamlUnassignedEvent>),
    Assigned(Box<DamlAssignedEvent>),
}

impl TryFrom<ReassignmentEvent> for DamlReassignmentEvent {
    type Error = DamlError;

    fn try_from(e: ReassignmentEvent) -> DamlResult<Self> {
        Ok(match e.event.req()? {
            ReassignmentEventKind::Unassigned(e) => Self::Unassigned(Box::new(DamlUnassignedEvent::try_from(e)?)),
            ReassignmentEventKind::Assigned(e) => Self::Assigned(Box::new(DamlAssignedEvent::try_from(e)?)),
        })
    }
}

/// Records that a contract was unassigned on its source synchronizer
/// and made unusable there pending a matching `Assigned` on the target.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlUnassignedEvent {
    /// Use this id as the `reassignment_id` in a follow-up
    /// `DamlAssignCommand` to complete the move.
    pub reassignment_id: String,
    pub contract_id: String,
    pub template_id: DamlIdentifier,
    pub source: String,
    pub target: String,
    /// The submitting party, or empty if the unassignment happened
    /// via the offline repair service.
    pub submitter: String,
    /// Same on the matching `Assigned` event; strictly increases with
    /// each unassign for the same contract; `0` for the original
    /// creation.
    pub reassignment_counter: u64,
    /// Until this time on the target synchronizer, only the submitter
    /// can issue the matching `Assign`. After that, any participant
    /// can. `None` when not applicable.
    pub assignment_exclusivity: Option<DateTime<Utc>>,
    pub witness_parties: Vec<String>,
    pub package_name: String,
    pub offset: DamlLedgerOffset,
    pub node_id: i32,
}

impl TryFrom<UnassignedEvent> for DamlUnassignedEvent {
    type Error = DamlError;

    fn try_from(e: UnassignedEvent) -> DamlResult<Self> {
        Ok(Self {
            reassignment_id: e.reassignment_id,
            contract_id: e.contract_id,
            template_id: DamlIdentifier::from(e.template_id.req()?),
            source: e.source,
            target: e.target,
            submitter: e.submitter,
            reassignment_counter: e.reassignment_counter,
            assignment_exclusivity: e.assignment_exclusivity.as_ref().map(util::from_grpc_timestamp).transpose()?,
            witness_parties: e.witness_parties,
            package_name: e.package_name,
            offset: DamlLedgerOffset::new(e.offset),
            node_id: e.node_id,
        })
    }
}

/// Records that a previously-unassigned contract was assigned on its
/// target synchronizer, making it usable there. Carries a
/// `CreatedEvent` that materialises the contract on the target.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlAssignedEvent {
    pub source: String,
    pub target: String,
    /// Matches the `reassignment_id` of the `Unassigned` half.
    pub reassignment_id: String,
    pub submitter: String,
    pub reassignment_counter: u64,
    /// The contract as it appears on the target synchronizer. The
    /// event's `offset` is the assignment offset; `node_id` is the
    /// index within the assignment batch.
    pub created_event: DamlCreatedEvent,
}

impl TryFrom<AssignedEvent> for DamlAssignedEvent {
    type Error = DamlError;

    fn try_from(e: AssignedEvent) -> DamlResult<Self> {
        Ok(Self {
            source: e.source,
            target: e.target,
            reassignment_id: e.reassignment_id,
            submitter: e.submitter,
            reassignment_counter: e.reassignment_counter,
            created_event: DamlCreatedEvent::try_from(e.created_event.req()?)?,
        })
    }
}

// ---------------------------------------------------------------------------
// Request-side: ReassignmentCommands + Unassign/Assign commands
// ---------------------------------------------------------------------------

/// A batch of reassignment commands processed atomically.
///
/// Unlike [`DamlCommands`](crate::data::DamlCommands), reassignment
/// submissions act on behalf of a *single* `submitter` (act-as/read-as
/// don't apply — reassignments are single-party operations).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DamlReassignmentCommands {
    pub workflow_id: String,
    /// Same semantics as `Commands::user_id`: ignored when the request
    /// is authenticated with a user token (the token's `user_id` wins).
    pub user_id: String,
    pub command_id: String,
    pub submitter: String,
    /// UUID per submission attempt to disambiguate retries with the
    /// same change-id. Empty to let the participant pick.
    pub submission_id: String,
    /// Must be non-empty.
    pub commands: Vec<DamlReassignmentCommand>,
}

impl From<DamlReassignmentCommands> for ReassignmentCommands {
    fn from(c: DamlReassignmentCommands) -> Self {
        Self {
            workflow_id: c.workflow_id,
            user_id: c.user_id,
            command_id: c.command_id,
            submitter: c.submitter,
            submission_id: c.submission_id,
            commands: c.commands.into_iter().map(Into::into).collect(),
        }
    }
}

/// One command in a [`DamlReassignmentCommands`] batch — either an
/// `Unassign` (move out of a synchronizer) or an `Assign` (complete a
/// prior unassignment on the target).
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DamlReassignmentCommand {
    Unassign(DamlUnassignCommand),
    Assign(DamlAssignCommand),
}

impl From<DamlReassignmentCommand> for ReassignmentCommand {
    fn from(c: DamlReassignmentCommand) -> Self {
        Self {
            command: Some(match c {
                DamlReassignmentCommand::Unassign(u) => ReassignmentCommandKind::UnassignCommand(u.into()),
                DamlReassignmentCommand::Assign(a) => ReassignmentCommandKind::AssignCommand(a.into()),
            }),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlUnassignCommand {
    pub contract_id: String,
    pub source: String,
    pub target: String,
}

impl From<DamlUnassignCommand> for UnassignCommand {
    fn from(c: DamlUnassignCommand) -> Self {
        Self {
            contract_id: c.contract_id,
            source: c.source,
            target: c.target,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlAssignCommand {
    /// Must match the `reassignment_id` from the matching
    /// `Unassigned` event.
    pub reassignment_id: String,
    pub source: String,
    pub target: String,
}

impl From<DamlAssignCommand> for AssignCommand {
    fn from(c: DamlAssignCommand) -> Self {
        Self {
            reassignment_id: c.reassignment_id,
            source: c.source,
            target: c.target,
        }
    }
}
