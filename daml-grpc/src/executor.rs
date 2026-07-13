use async_trait::async_trait;

use crate::data::command::{DamlCommand, DamlCreateCommand, DamlExerciseCommand};
use crate::data::event::{DamlCreatedEvent, DamlEvent};
use crate::data::filter::{DamlEventFormat, DamlTransactionFormat, DamlTransactionShape};
use crate::data::value::DamlValue;
use crate::data::{DamlCommandsDeduplicationPeriod, DamlError, DamlMinLedgerTime, DamlResult, DamlTransaction};
use crate::service::DamlCommandService;
use crate::util::Required;
use crate::{DamlCommandFactory, DamlGrpcClient};

/// Construct a [`DamlSimpleExecutor`].
pub struct DamlSimpleExecutorBuilder<'a> {
    ledger_client: &'a DamlGrpcClient,
    act_as: Option<Vec<String>>,
    read_as: Option<Vec<String>>,
    workflow_id: Option<&'a str>,
    user_id: Option<&'a str>,
    deduplication_period: Option<DamlCommandsDeduplicationPeriod>,
    min_ledger_time: Option<DamlMinLedgerTime>,
    auth_token: Option<&'a str>,
}

impl<'a> DamlSimpleExecutorBuilder<'a> {
    pub const fn new(ledger_client: &'a DamlGrpcClient) -> Self {
        Self {
            ledger_client,
            act_as: None,
            read_as: None,
            workflow_id: None,
            user_id: None,
            deduplication_period: None,
            min_ledger_time: None,
            auth_token: None,
        }
    }

    pub fn workflow_id(self, workflow_id: &'a str) -> Self {
        Self {
            workflow_id: Some(workflow_id),
            ..self
        }
    }

    pub fn act_as(self, act_as: impl Into<String>) -> Self {
        Self {
            act_as: Some(vec![act_as.into()]),
            ..self
        }
    }

    pub fn act_as_all(self, act_as_all: Vec<String>) -> Self {
        Self {
            act_as: Some(act_as_all),
            ..self
        }
    }

    pub fn read_as(self, read_as: impl Into<String>) -> Self {
        Self {
            read_as: Some(vec![read_as.into()]),
            ..self
        }
    }

    pub fn read_as_all(self, read_as_all: Vec<String>) -> Self {
        Self {
            read_as: Some(read_as_all),
            ..self
        }
    }

    /// v2 wire name (v1's `application_id`). Sets the submission's
    /// `user_id`, which the participant matches against the JWT's
    /// `sub` claim when the auth layer is user-based.
    pub fn user_id(self, user_id: &'a str) -> Self {
        Self {
            user_id: Some(user_id),
            ..self
        }
    }

    pub fn deduplication_period(self, deduplication_period: DamlCommandsDeduplicationPeriod) -> Self {
        Self {
            deduplication_period: Some(deduplication_period),
            ..self
        }
    }

    pub fn min_ledger_time(self, min_ledger_time: DamlMinLedgerTime) -> Self {
        Self {
            min_ledger_time: Some(min_ledger_time),
            ..self
        }
    }

    /// Override any JWT token enabled in the `DamlGrpcClient`.
    pub fn auth_token(self, auth_token: &'a str) -> Self {
        Self {
            auth_token: Some(auth_token),
            ..self
        }
    }

    pub fn build(self) -> DamlResult<DamlSimpleExecutor<'a>> {
        if self.has_parties() {
            Ok(DamlSimpleExecutor::new(
                self.ledger_client,
                self.act_as.unwrap_or_default(),
                self.read_as.unwrap_or_default(),
                self.workflow_id.unwrap_or("default-workflow"),
                self.user_id.unwrap_or("default-user"),
                self.deduplication_period,
                self.min_ledger_time,
                self.auth_token,
            ))
        } else {
            Err(DamlError::InsufficientParties)
        }
    }

    fn has_parties(&self) -> bool {
        match (self.act_as.as_deref(), self.read_as.as_deref()) {
            (None, None) => false,
            (Some(act_as), None) => !act_as.is_empty(),
            (None, Some(read_as)) => !read_as.is_empty(),
            (Some(act_as), Some(read_as)) => !act_as.is_empty() || !read_as.is_empty(),
        }
    }
}

/// An async failable Daml command executor.
///
/// v2 removed the dedicated `TransactionTree` response shape: the
/// tree-shaped (ledger-effects) view is now selectable on the same
/// `SubmitAndWaitForTransaction` RPC via a `TransactionFormat`
/// whose `transaction_shape = LedgerEffects`. The executor exposes
/// that selection through [`Self::execute_for_transaction_with_effects`]
/// which returns the same `DamlTransaction` populated with both
/// `Created` and `Exercised` events.
#[async_trait]
pub trait CommandExecutor {
    async fn execute_for_transaction(&self, command: DamlCommand) -> DamlResult<DamlTransaction>;
    async fn execute_for_transaction_with_effects(&self, command: DamlCommand) -> DamlResult<DamlTransaction>;
    async fn execute_create(&self, create_command: DamlCreateCommand) -> DamlResult<DamlCreatedEvent>;
    async fn execute_exercise(&self, exercise_command: DamlExerciseCommand) -> DamlResult<DamlValue>;
}

/// A simple async Daml command executor.
pub struct DamlSimpleExecutor<'a> {
    ledger_client: &'a DamlGrpcClient,
    command_factory: DamlCommandFactory,
    auth_token: Option<&'a str>,
}

impl<'a> DamlSimpleExecutor<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ledger_client: &'a DamlGrpcClient,
        act_as: Vec<String>,
        read_as: Vec<String>,
        workflow_id: &str,
        user_id: &str,
        deduplication_period: Option<DamlCommandsDeduplicationPeriod>,
        min_ledger_time: Option<DamlMinLedgerTime>,
        auth_token: Option<&'a str>,
    ) -> Self {
        let command_factory =
            DamlCommandFactory::new(workflow_id, user_id, act_as, read_as, deduplication_period, min_ledger_time);
        Self {
            ledger_client,
            command_factory,
            auth_token,
        }
    }

    pub fn act_as(&self) -> &[String] {
        self.command_factory.act_as()
    }

    pub fn read_as(&self) -> &[String] {
        self.command_factory.read_as()
    }

    async fn submit_and_wait_for_transaction(&self, command: DamlCommand) -> DamlResult<DamlTransaction> {
        let commands = self.command_factory.make_command(command);
        // Default `None` transaction-format selects ACS-delta shape
        // with per-party wildcard filters — fine for "give me what I
        // just submitted" use.
        self.client().submit_and_wait_for_transaction(commands, None).await
    }

    /// Submit and wait, returning a [`DamlTransaction`] populated
    /// with both `Created` and `Exercised` events (the `LedgerEffects`
    /// shape; v1's `TransactionTree`).
    async fn submit_and_wait_for_transaction_with_effects(&self, command: DamlCommand) -> DamlResult<DamlTransaction> {
        let commands = self.command_factory.make_command(command);
        // Build a transaction-format scoped to the submitter's parties
        // with the LedgerEffects shape and verbose output.
        let mut filters_by_party = std::collections::HashMap::new();
        let wildcard = crate::data::filter::DamlFilters::default();
        for party in self.act_as() {
            filters_by_party.insert(party.clone(), wildcard.clone());
        }
        for party in self.read_as() {
            filters_by_party.insert(party.clone(), wildcard.clone());
        }
        let event_format = DamlEventFormat {
            filters_by_party,
            filters_for_any_party: None,
            verbose: true,
        };
        let format = DamlTransactionFormat {
            event_format,
            transaction_shape: DamlTransactionShape::LedgerEffects,
        };
        self.client().submit_and_wait_for_transaction(commands, Some(format)).await
    }

    fn client(&self) -> DamlCommandService<'_> {
        match self.auth_token {
            Some(token) => self.ledger_client.command_service().with_token(token),
            None => self.ledger_client.command_service(),
        }
    }
}

#[async_trait]
#[allow(clippy::needless_lifetimes)]
impl CommandExecutor for DamlSimpleExecutor<'_> {
    async fn execute_for_transaction(&self, command: DamlCommand) -> DamlResult<DamlTransaction> {
        self.submit_and_wait_for_transaction(command).await
    }

    async fn execute_for_transaction_with_effects(&self, command: DamlCommand) -> DamlResult<DamlTransaction> {
        self.submit_and_wait_for_transaction_with_effects(command).await
    }

    async fn execute_create(&self, create_command: DamlCreateCommand) -> Result<DamlCreatedEvent, DamlError> {
        let mut tx = self.submit_and_wait_for_transaction(DamlCommand::Create(create_command)).await?;
        if tx.events.is_empty() {
            return Err(DamlError::Other("execute_create: transaction had no events".to_owned()));
        }
        tx.events.swap_remove(0).try_created()
    }

    /// Submit an exercise command and return the result of the first
    /// `Exercised` event whose `exercise_result` is populated.
    ///
    /// Returns [`DamlError::MissingRequiredField`] if the transaction
    /// contains no `Exercised` event (e.g. a mis-routed `Create`
    /// command) or if every `Exercised` event has an empty result. In
    /// practice the participant populates `exercise_result` for every
    /// choice — even non-consuming, unit-returning ones (as
    /// `Some(DamlValue::Unit)`) — so this path only fires for
    /// malformed responses.
    async fn execute_exercise(&self, exercise_command: DamlExerciseCommand) -> Result<DamlValue, DamlError> {
        let tx = self.submit_and_wait_for_transaction_with_effects(DamlCommand::Exercise(exercise_command)).await?;
        tx.events
            .into_iter()
            .find_map(|e| match e {
                DamlEvent::Exercised(ex) => ex.exercise_result.clone(),
                _ => None,
            })
            .req()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_act_as() -> DamlResult<()> {
        let client = DamlGrpcClient::dummy_for_testing();
        let executor = DamlSimpleExecutorBuilder::new(&client).act_as("Alice").build()?;
        assert_eq!(&["Alice"], executor.act_as());
        assert_eq!(0, executor.read_as().len());
        Ok(())
    }

    #[tokio::test]
    async fn test_read_as() -> DamlResult<()> {
        let client = DamlGrpcClient::dummy_for_testing();
        let executor = DamlSimpleExecutorBuilder::new(&client).read_as("Alice").build()?;
        assert_eq!(&["Alice"], executor.read_as());
        assert_eq!(0, executor.act_as().len());
        Ok(())
    }

    #[tokio::test]
    async fn test_act_as_and_read_as() -> DamlResult<()> {
        let client = DamlGrpcClient::dummy_for_testing();
        let executor = DamlSimpleExecutorBuilder::new(&client).act_as("Alice").read_as("Bob").build()?;
        assert_eq!(&["Alice"], executor.act_as());
        assert_eq!(&["Bob"], executor.read_as());
        Ok(())
    }

    #[tokio::test]
    async fn test_act_as_all() -> DamlResult<()> {
        let client = DamlGrpcClient::dummy_for_testing();
        let executor =
            DamlSimpleExecutorBuilder::new(&client).act_as_all(vec!["Alice".into(), "Bob".into()]).build()?;
        assert_eq!(&["Alice", "Bob"], executor.act_as());
        assert_eq!(0, executor.read_as().len());
        Ok(())
    }

    #[tokio::test]
    async fn test_read_as_all() -> DamlResult<()> {
        let client = DamlGrpcClient::dummy_for_testing();
        let executor =
            DamlSimpleExecutorBuilder::new(&client).read_as_all(vec!["Alice".into(), "Bob".into()]).build()?;
        assert_eq!(&["Alice", "Bob"], executor.read_as());
        assert_eq!(0, executor.act_as().len());
        Ok(())
    }

    #[tokio::test]
    async fn test_act_as_all_and_read_as_all() -> DamlResult<()> {
        let client = DamlGrpcClient::dummy_for_testing();
        let executor = DamlSimpleExecutorBuilder::new(&client)
            .act_as_all(vec!["Alice".into(), "Bob".into()])
            .read_as_all(vec!["John".into(), "Jill".into()])
            .build()?;
        assert_eq!(&["Alice", "Bob"], executor.act_as());
        assert_eq!(&["John", "Jill"], executor.read_as());
        Ok(())
    }

    #[tokio::test]
    async fn test_no_actors_should_fail() -> DamlResult<()> {
        let client = DamlGrpcClient::dummy_for_testing();
        let executor = DamlSimpleExecutorBuilder::new(&client).build();
        match executor {
            Err(DamlError::InsufficientParties) => (),
            _ => panic!("expected DamlError::InsufficientParties"),
        }
        Ok(())
    }
}
