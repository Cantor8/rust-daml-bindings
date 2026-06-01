use std::convert::TryFrom;
use std::fmt::Debug;

use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::offset::DamlLedgerOffset;
use crate::data::{DamlCommands, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::command_service_client::CommandServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{Commands, SubmitAndWaitRequest};
use crate::service::common::make_request;

/// Submit a composite command to a v2 participant and wait synchronously
/// for the participant's verdict (success or rejection).
///
/// v2 reshaped the v1 surface:
/// - `SubmitAndWait` now returns the `update_id` and `completion_offset`
///   (v1 had a separate `SubmitAndWaitForTransactionId` RPC for that —
///   the consolidated response makes that variant redundant).
/// - `SubmitAndWaitForTransactionTree` is gone; the tree shape is now
///   selectable via `TransactionFormat` on the transaction-returning
///   variant.
/// - A new `SubmitAndWaitForReassignment` covers cross-synchronizer
///   contract movements.
///
/// This checkpoint wires up only `submit_and_wait`. The two
/// transaction- and reassignment-returning variants need the new
/// `Transaction` / `Reassignment` wrappers, which come with the
/// UpdateService and reassignment checkpoints respectively.
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
        let response =
            self.client().submit_and_wait(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlSubmitAndWaitOutcome {
            update_id: response.update_id,
            completion_offset: DamlLedgerOffset::new(response.completion_offset),
        })
    }

    fn client(&self) -> CommandServiceClient<Channel> {
        CommandServiceClient::new(self.channel.clone())
    }
}
