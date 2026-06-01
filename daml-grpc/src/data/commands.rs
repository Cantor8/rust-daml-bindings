use std::convert::TryFrom;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::data::command::DamlCommand;
use crate::data::identifier::DamlIdentifier;
use crate::data::value::DamlValue;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::commands::DeduplicationPeriod;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{Command, Commands, DisclosedContract, PrefetchContractKey};
use crate::util;

/// A composite Daml command: a set of [`DamlCommand`]s that the participant
/// processes atomically (succeed-all-or-fail-all). Plus metadata about
/// who submitted them, dedup period, and disclosed-contract overrides.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlCommands {
    /// On-ledger workflow identifier. Optional in v2 — leave empty when
    /// not tracking workflows.
    pub workflow_id: String,
    /// The participant user that issued this submission. Required unless
    /// the request is already authenticated with a user token (in which
    /// case the token's user_id takes precedence and this field is
    /// ignored). v2 renames v1's `application_id`.
    pub user_id: String,
    /// Uniquely identifies this command (together with `user_id` and
    /// `act_as`). Required.
    pub command_id: String,
    /// Disambiguates retries of the same change-id. Typically a fresh
    /// UUID per submission attempt. Optional — the participant fills in a
    /// value if you leave it empty.
    pub submission_id: String,
    /// Set of parties this submission acts on behalf of. Must be
    /// non-empty.
    pub act_as: Vec<String>,
    /// Additional parties whose contracts can be read but not signed for.
    pub read_as: Vec<String>,
    /// The atomic command list itself. Must be non-empty.
    pub commands: Vec<DamlCommand>,
    /// Deduplication period, by duration or completion-stream offset.
    /// Defaults to the participant's configured maximum if unset.
    pub deduplication_period: Option<DamlCommandsDeduplicationPeriod>,
    /// Lower bound on the ledger time of the resulting transaction.
    pub min_ledger_time: Option<DamlMinLedgerTime>,
    /// Disclosed contracts allow the submitter to authoritatively present
    /// contracts the participant might not otherwise know about, e.g.
    /// from upstream events on other synchronizers.
    pub disclosed_contracts: Vec<DamlDisclosedContract>,
    /// Target synchronizer id. Leave empty to let the participant route.
    pub synchronizer_id: String,
    /// Package-name -> package-id pinning hints for command interpretation.
    pub package_id_selection_preference: Vec<String>,
    /// Keys to prefetch into participant caches before interpretation.
    pub prefetch_contract_keys: Vec<DamlPrefetchContractKey>,
    /// Caps the number of topology-aware package selection passes the
    /// participant performs. `None` defers to participant config.
    pub taps_max_passes: Option<u32>,
}

impl DamlCommands {
    /// Minimal constructor for the common case: a set of commands acting
    /// as a single party with no extra hints. All other fields default
    /// to empty / unset; use struct update syntax to refine.
    pub fn new(
        user_id: impl Into<String>,
        command_id: impl Into<String>,
        act_as: impl Into<Vec<String>>,
        commands: impl Into<Vec<DamlCommand>>,
    ) -> Self {
        Self {
            workflow_id: String::new(),
            user_id: user_id.into(),
            command_id: command_id.into(),
            submission_id: String::new(),
            act_as: act_as.into(),
            read_as: Vec::new(),
            commands: commands.into(),
            deduplication_period: None,
            min_ledger_time: None,
            disclosed_contracts: Vec::new(),
            synchronizer_id: String::new(),
            package_id_selection_preference: Vec::new(),
            prefetch_contract_keys: Vec::new(),
            taps_max_passes: None,
        }
    }
}

impl TryFrom<DamlCommands> for Commands {
    type Error = DamlError;

    fn try_from(d: DamlCommands) -> DamlResult<Commands> {
        Ok(Commands {
            workflow_id: d.workflow_id,
            user_id: d.user_id,
            command_id: d.command_id,
            submission_id: d.submission_id,
            act_as: d.act_as,
            read_as: d.read_as,
            commands: d.commands.into_iter().map(Command::from).collect(),
            min_ledger_time_abs: match &d.min_ledger_time {
                Some(DamlMinLedgerTime::Absolute(ts)) => Some(util::to_grpc_timestamp(*ts)?),
                _ => None,
            },
            min_ledger_time_rel: match &d.min_ledger_time {
                Some(DamlMinLedgerTime::Relative(dur)) => Some(util::to_grpc_duration(dur)?),
                _ => None,
            },
            deduplication_period: d.deduplication_period.map(DeduplicationPeriod::try_from).transpose()?,
            disclosed_contracts: d.disclosed_contracts.into_iter().map(DisclosedContract::from).collect(),
            synchronizer_id: d.synchronizer_id,
            package_id_selection_preference: d.package_id_selection_preference,
            prefetch_contract_keys: d
                .prefetch_contract_keys
                .into_iter()
                .map(PrefetchContractKey::try_from)
                .collect::<DamlResult<Vec<_>>>()?,
            taps_max_passes: d.taps_max_passes,
        })
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlMinLedgerTime {
    Absolute(DateTime<Utc>),
    Relative(Duration),
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlCommandsDeduplicationPeriod {
    /// Offset on the completion stream, exclusive. v2 makes this a real
    /// integer offset rather than v1's stringified form.
    DeduplicationOffset(i64),
    DeduplicationDuration(Duration),
}

impl TryFrom<DamlCommandsDeduplicationPeriod> for DeduplicationPeriod {
    type Error = DamlError;

    fn try_from(period: DamlCommandsDeduplicationPeriod) -> DamlResult<Self> {
        Ok(match period {
            DamlCommandsDeduplicationPeriod::DeduplicationOffset(offset) =>
                DeduplicationPeriod::DeduplicationOffset(offset),
            DamlCommandsDeduplicationPeriod::DeduplicationDuration(dur) =>
                DeduplicationPeriod::DeduplicationDuration(util::to_grpc_duration(&dur)?),
        })
    }
}

/// An out-of-band contract the submitter is asserting exists on the
/// network, presented to the participant alongside a command so it can
/// be referenced even when the participant hasn't otherwise observed it.
///
/// `created_event_blob` is the authoritative payload — the optional
/// `template_id` and `contract_id` are validated against the blob when
/// supplied.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlDisclosedContract {
    pub template_id: Option<DamlIdentifier>,
    pub contract_id: String,
    pub created_event_blob: Vec<u8>,
    pub synchronizer_id: String,
}

impl From<DamlDisclosedContract> for DisclosedContract {
    fn from(d: DamlDisclosedContract) -> Self {
        Self {
            template_id: d.template_id.map(Into::into),
            contract_id: d.contract_id,
            created_event_blob: d.created_event_blob,
            synchronizer_id: d.synchronizer_id,
        }
    }
}

/// Hints to the participant that it should warm its caches with contracts
/// indexed by `(template_id, contract_key)` before interpreting the
/// commands. `limit = None` means "fetch one"; `Some(0)` is forbidden by
/// the protocol.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlPrefetchContractKey {
    pub template_id: DamlIdentifier,
    pub contract_key: DamlValue,
    pub limit: Option<u32>,
}

impl TryFrom<DamlPrefetchContractKey> for PrefetchContractKey {
    type Error = DamlError;

    fn try_from(p: DamlPrefetchContractKey) -> DamlResult<Self> {
        Ok(Self {
            template_id: Some(p.template_id.into()),
            contract_key: Some(p.contract_key.into()),
            limit: p.limit,
        })
    }
}
