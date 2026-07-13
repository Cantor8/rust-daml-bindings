use std::convert::TryFrom;
use std::fmt::Debug;

use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::DamlTransaction;
use crate::data::filter::{DamlEventFormat, DamlTransactionFormat};
use crate::data::offset::DamlLedgerOffset;
use crate::data::reassignment::{DamlReassignment, DamlReassignmentCommands};
use crate::data::{DamlCommands, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::command_service_client::CommandServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{
    Commands, ReassignmentCommands, SubmitAndWaitForReassignmentRequest, SubmitAndWaitForTransactionRequest,
    SubmitAndWaitRequest,
};
use crate::service::common::make_request;
use crate::util::Required;

/// Submit a composite command to a v2 participant and wait synchronously
/// for the participant's verdict (success or rejection).
///
/// v2 reshaped the v1 surface:
/// - `SubmitAndWait` returns the `update_id` and `completion_offset`
///   (v1 had a separate `SubmitAndWaitForTransactionId` RPC for that —
///   the consolidated response makes that variant redundant).
/// - `SubmitAndWaitForTransaction` now takes an optional
///   `TransactionFormat` that selects shape, parties, and the
///   `verbose` flag.
/// - `SubmitAndWaitForTransactionTree` is gone; the tree shape is now
///   selectable via `TransactionFormat::transaction_shape =
///   LedgerEffects`.
/// - A new `SubmitAndWaitForReassignment` covers cross-synchronizer
///   contract movements.
#[derive(Debug)]
pub struct DamlCommandService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

/// The outcome of a successful synchronous submission: the resulting
/// update's id (a transaction id or a reassignment id) and the
/// completion-stream offset at which the participant emitted the
/// matching completion.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DamlSubmitAndWaitOutcome {
    pub update_id: String,
    pub completion_offset: DamlLedgerOffset,
}

impl<'a> DamlCommandService<'a> {
    pub fn new(channel: Channel, auth_token: Option<&'a str>) -> Self {
        Self {
            channel,
            auth_token,
        }
    }

    /// Override the JWT token to use for this service.
    pub fn with_token(self, auth_token: &'a str) -> Self {
        Self {
            auth_token: Some(auth_token),
            ..self
        }
    }

    /// Submit a [`DamlCommands`] payload and wait for the participant's
    /// completion verdict.
    ///
    /// Returns the `update_id` and `completion_offset` of the resulting
    /// update. Propagates Daml interpretation errors and other
    /// rejections as gRPC errors (unlike the asynchronous
    /// `CommandSubmissionService.Submit`, which only signals delivery).
    #[instrument(skip(self))]
    pub async fn submit_and_wait(
        &self,
        commands: impl Into<DamlCommands> + Debug,
    ) -> DamlResult<DamlSubmitAndWaitOutcome> {
        let payload = SubmitAndWaitRequest {
            commands: Some(Commands::try_from(commands.into())?),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().submit_and_wait(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlSubmitAndWaitOutcome {
            update_id: response.update_id,
            completion_offset: DamlLedgerOffset::new(response.completion_offset),
        })
    }

    /// Submit a [`DamlCommands`] payload and wait for the resulting
    /// transaction.
    ///
    /// `transaction_format` selects the event shape, the per-party
    /// filters, and the `verbose` flag. Passing `None` lets the
    /// participant pick a default: `transaction_shape = AcsDelta`,
    /// `event_format` with wildcard-template filters for every
    /// `act_as` and `read_as` party in the submission, `verbose = true`.
    /// That default is fine for "give me what I just submitted" use
    /// cases; build a custom `DamlTransactionFormat` to switch shapes
    /// or scope to specific parties.
    #[instrument(skip(self))]
    pub async fn submit_and_wait_for_transaction(
        &self,
        commands: impl Into<DamlCommands> + Debug,
        transaction_format: Option<DamlTransactionFormat>,
    ) -> DamlResult<DamlTransaction> {
        let payload = SubmitAndWaitForTransactionRequest {
            commands: Some(Commands::try_from(commands.into())?),
            transaction_format: transaction_format.map(Into::into),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response =
            self.client().submit_and_wait_for_transaction(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        DamlTransaction::try_from(response.transaction.req()?)
    }

    /// Submit a [`DamlReassignmentCommands`] payload and wait for the
    /// resulting reassignment.
    ///
    /// `event_format` controls which events appear in the returned
    /// `DamlReassignment`. Passing `None` returns a reassignment with
    /// no events populated — useful when the caller only cares about
    /// the `update_id` and offset. The events themselves take ACS-delta
    /// shape regardless of any `transaction_shape` you might set
    /// elsewhere; reassignments have no notion of a tree.
    #[instrument(skip(self))]
    pub async fn submit_and_wait_for_reassignment(
        &self,
        commands: impl Into<DamlReassignmentCommands> + Debug,
        event_format: Option<DamlEventFormat>,
    ) -> DamlResult<DamlReassignment> {
        let payload = SubmitAndWaitForReassignmentRequest {
            reassignment_commands: Some(ReassignmentCommands::from(commands.into())),
            event_format: event_format.map(Into::into),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response =
            self.client().submit_and_wait_for_reassignment(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        DamlReassignment::try_from(response.reassignment.req()?)
    }

    fn client(&self) -> CommandServiceClient<Channel> {
        CommandServiceClient::new(self.channel.clone())
    }
}
