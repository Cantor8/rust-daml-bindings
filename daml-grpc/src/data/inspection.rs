use std::convert::TryFrom;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::data::command::DamlCommand;
use crate::data::completion::DamlCompletion;
use crate::data::identifier::DamlIdentifier;
use crate::data::value::DamlValue;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::{
    CommandState, CommandStatus, CommandUpdates, Contract, RequestStatistics, Timing,
};
use crate::util;
use crate::util::Required;

/// Lifecycle state of a command on the participant.
///
/// `Unspecified` is the proto's wire-default and is also used as a
/// query-wildcard ("match any state"). Production code typically only
/// inspects `Pending` / `Succeeded` / `Failed`.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum DamlCommandState {
    Unspecified,
    Pending,
    Succeeded,
    Failed,
}

impl From<CommandState> for DamlCommandState {
    fn from(s: CommandState) -> Self {
        match s {
            CommandState::Unspecified => Self::Unspecified,
            CommandState::Pending => Self::Pending,
            CommandState::Succeeded => Self::Succeeded,
            CommandState::Failed => Self::Failed,
        }
    }
}

impl From<DamlCommandState> for CommandState {
    fn from(s: DamlCommandState) -> Self {
        match s {
            DamlCommandState::Unspecified => Self::Unspecified,
            DamlCommandState::Pending => Self::Pending,
            DamlCommandState::Succeeded => Self::Succeeded,
            DamlCommandState::Failed => Self::Failed,
        }
    }
}

/// One stage in a command's processing pipeline, with its duration.
/// Used by the inspection service to surface per-stage timing — the
/// stage descriptions are participant-implementation-specific and may
/// change between releases.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlTiming {
    pub description: String,
    pub duration: Duration,
}

impl From<Timing> for DamlTiming {
    fn from(t: Timing) -> Self {
        Self {
            description: t.description,
            duration: Duration::from_millis(u64::from(t.duration_ms)),
        }
    }
}

/// Per-command request statistics surfaced by the inspection service.
/// All fields are `Optional` on the wire; missing fields decode as 0.
#[derive(Debug, Eq, PartialEq, Clone, Copy, Default)]
pub struct DamlRequestStatistics {
    pub envelopes: u32,
    pub request_size: u32,
    pub recipients: u32,
}

impl From<RequestStatistics> for DamlRequestStatistics {
    fn from(s: RequestStatistics) -> Self {
        Self {
            envelopes: s.envelopes,
            request_size: s.request_size,
            recipients: s.recipients,
        }
    }
}

/// Aggregate of the ledger updates effected by a command.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlCommandUpdates {
    pub created: Vec<DamlInspectedContract>,
    pub archived: Vec<DamlInspectedContract>,
    pub exercised: u32,
    pub fetched: u32,
    pub looked_up_by_key: u32,
}

impl TryFrom<CommandUpdates> for DamlCommandUpdates {
    type Error = DamlError;

    fn try_from(u: CommandUpdates) -> DamlResult<Self> {
        Ok(Self {
            created: u.created.into_iter().map(DamlInspectedContract::try_from).collect::<DamlResult<_>>()?,
            archived: u.archived.into_iter().map(DamlInspectedContract::try_from).collect::<DamlResult<_>>()?,
            exercised: u.exercised,
            fetched: u.fetched,
            looked_up_by_key: u.looked_up_by_key,
        })
    }
}

/// The inspection service's lightweight contract view — a
/// (template, id, optional key) triple. Distinct from the heavier
/// event-stream contract types because inspection only surfaces
/// debug-grade metadata.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlInspectedContract {
    pub template_id: DamlIdentifier,
    pub contract_id: String,
    pub contract_key: Option<DamlValue>,
}

impl TryFrom<Contract> for DamlInspectedContract {
    type Error = DamlError;

    fn try_from(c: Contract) -> DamlResult<Self> {
        Ok(Self {
            template_id: DamlIdentifier::from(c.template_id.req()?),
            contract_id: c.contract_id,
            contract_key: c.contract_key.map(DamlValue::try_from).transpose()?,
        })
    }
}

/// The inspection-service view of a single command's status.
///
/// `completion` is only meaningful once `state` leaves `Pending`;
/// while pending the inner fields hold proto-default values. Several
/// fields are optional on the wire and surface as `None` /
/// `Default::default()` here.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlCommandStatus {
    /// Time at which the participant accepted the submission for
    /// interpretation.
    pub started: DateTime<Utc>,
    /// Time at which the participant finished interpreting and
    /// committing (or rejecting) the command. `None` while pending.
    pub completed: Option<DateTime<Utc>>,
    /// The completion that the CompletionService would emit for this
    /// command. Only populated once `state` leaves `Pending`.
    pub completion: Option<DamlCompletion>,
    pub state: DamlCommandState,
    /// The individual submitted commands. Always non-empty.
    pub commands: Vec<DamlCommand>,
    pub request_statistics: Option<DamlRequestStatistics>,
    /// The ledger updates effected by the command. `None` while
    /// pending or for failed commands that produced no updates.
    pub updates: Option<DamlCommandUpdates>,
    /// Synchronizer the command was routed to. Empty while the
    /// participant hasn't yet decided on a routing target.
    pub synchronizer_id: String,
    /// Per-stage timing breakdown.
    pub timings: Vec<DamlTiming>,
}

impl TryFrom<CommandStatus> for DamlCommandStatus {
    type Error = DamlError;

    fn try_from(s: CommandStatus) -> DamlResult<Self> {
        Ok(Self {
            started: util::from_grpc_timestamp(&s.started.req()?),
            completed: s.completed.as_ref().map(util::from_grpc_timestamp),
            completion: s.completion.map(DamlCompletion::try_from).transpose()?,
            state: DamlCommandState::from(CommandState::from_i32(s.state).req()?),
            commands: s.commands.into_iter().map(DamlCommand::try_from).collect::<DamlResult<_>>()?,
            request_statistics: s.request_statistics.map(DamlRequestStatistics::from),
            updates: s.updates.map(DamlCommandUpdates::try_from).transpose()?,
            synchronizer_id: s.synchronizer_id,
            timings: s.timings.into_iter().map(DamlTiming::from).collect(),
        })
    }
}
