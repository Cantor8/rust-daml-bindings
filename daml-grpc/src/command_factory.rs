use uuid::Uuid;

use crate::data::command::DamlCommand;
use crate::data::{DamlCommands, DamlCommandsDeduplicationPeriod, DamlMinLedgerTime};

/// Factory for assembling [`DamlCommands`] payloads from a fixed set of
/// submission parameters (workflow id, user id, act-as parties, dedup
/// period, …) and a variable list of [`DamlCommand`]s.
///
/// v2 dropped the single-party `party` field; submissions act on behalf
/// of the `act_as` set instead.
#[derive(Debug)]
pub struct DamlCommandFactory {
    workflow_id: String,
    user_id: String,
    act_as: Vec<String>,
    read_as: Vec<String>,
    deduplication_period: Option<DamlCommandsDeduplicationPeriod>,
    min_ledger_time: Option<DamlMinLedgerTime>,
}

impl DamlCommandFactory {
    pub fn new(
        workflow_id: impl Into<String>,
        user_id: impl Into<String>,
        act_as: impl Into<Vec<String>>,
        read_as: impl Into<Vec<String>>,
        deduplication_period: impl Into<Option<DamlCommandsDeduplicationPeriod>>,
        min_ledger_time: impl Into<Option<DamlMinLedgerTime>>,
    ) -> Self {
        Self {
            workflow_id: workflow_id.into(),
            user_id: user_id.into(),
            act_as: act_as.into(),
            read_as: read_as.into(),
            deduplication_period: deduplication_period.into(),
            min_ledger_time: min_ledger_time.into(),
        }
    }

    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn act_as(&self) -> &[String] {
        &self.act_as
    }

    pub fn read_as(&self) -> &[String] {
        &self.read_as
    }

    pub const fn deduplication_period(&self) -> &Option<DamlCommandsDeduplicationPeriod> {
        &self.deduplication_period
    }

    pub const fn min_ledger_time(&self) -> &Option<DamlMinLedgerTime> {
        &self.min_ledger_time
    }

    pub fn make_command(&self, command: DamlCommand) -> DamlCommands {
        self.make_commands::<String, _>(vec![command], None)
    }

    pub fn make_command_with_id(&self, command: DamlCommand, command_id: impl Into<String>) -> DamlCommands {
        self.make_commands(vec![command], Some(command_id))
    }

    pub fn make_commands<S, V>(&self, commands: V, command_id: Option<S>) -> DamlCommands
    where
        S: Into<String>,
        V: Into<Vec<DamlCommand>>,
    {
        DamlCommands {
            workflow_id: self.workflow_id.clone(),
            read_as: self.read_as.clone(),
            deduplication_period: self.deduplication_period.clone(),
            min_ledger_time: self.min_ledger_time.clone(),
            ..DamlCommands::new(
                self.user_id.clone(),
                command_id.map_or_else(|| Uuid::new_v4().to_string(), Into::into),
                self.act_as.clone(),
                commands,
            )
        }
    }
}
