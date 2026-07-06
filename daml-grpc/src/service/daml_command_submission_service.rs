use std::convert::TryFrom;
use std::fmt::Debug;

use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::reassignment::DamlReassignmentCommands;
use crate::data::{DamlCommands, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::command_submission_service_client::CommandSubmissionServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{
    Commands, ReassignmentCommands, SubmitReassignmentRequest, SubmitRequest,
};
use crate::service::common::make_request;

/// Advance the state of the ledger by submitting a [`DamlCommands`]
/// payload (one or more [`DamlCommand`]s plus their submission metadata)
/// or a [`DamlReassignmentCommands`] payload (cross-synchronizer
/// contract movements).
///
/// [`DamlCommand`]: crate::data::command::DamlCommand
#[derive(Debug)]
pub struct DamlCommandSubmissionService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

impl<'a> DamlCommandSubmissionService<'a> {
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

    /// Submit a [`DamlCommands`] payload. Returns the `command_id` from
    /// the submitted payload (echoed back for convenience — the
    /// participant signals completion asynchronously through the
    /// Completion stream, see `CommandCompletionService`).
    #[instrument(skip(self))]
    pub async fn submit_request(&self, commands: impl Into<DamlCommands> + Debug) -> DamlResult<String> {
        let commands = commands.into();
        let command_id = commands.command_id.clone();
        let payload = SubmitRequest {
            commands: Some(Commands::try_from(commands)?),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        self.client().submit(make_request(payload, self.auth_token)?).await?;
        trace!(?command_id);
        Ok(command_id)
    }

    /// Submit a [`DamlReassignmentCommands`] payload — a batch of
    /// `Unassign`/`Assign` operations processed atomically.
    /// Asynchronous like [`submit_request`](Self::submit_request): the
    /// participant's verdict is delivered through the completion
    /// stream, not in the RPC's response. Returns the submission's
    /// `command_id` for convenience.
    #[instrument(skip(self))]
    pub async fn submit_reassignment(
        &self,
        commands: impl Into<DamlReassignmentCommands> + Debug,
    ) -> DamlResult<String> {
        let commands = commands.into();
        let command_id = commands.command_id.clone();
        let payload = SubmitReassignmentRequest {
            reassignment_commands: Some(ReassignmentCommands::from(commands)),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        self.client().submit_reassignment(make_request(payload, self.auth_token)?).await?;
        trace!(?command_id);
        Ok(command_id)
    }

    fn client(&self) -> CommandSubmissionServiceClient<Channel> {
        CommandSubmissionServiceClient::new(self.channel.clone())
    }
}
