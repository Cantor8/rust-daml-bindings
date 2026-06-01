use std::convert::TryFrom;
use std::fmt::Debug;

use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::{DamlCommands, DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::command_submission_service_client::CommandSubmissionServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{Commands, SubmitRequest};
use crate::service::common::make_request;

/// Advance the state of the ledger by submitting a [`DamlCommands`]
/// payload (one or more [`DamlCommand`]s plus their submission metadata).
///
/// v2's `SubmitReassignment` RPC is intentionally not exposed here yet;
/// it requires the reassignment command tree which is wired up in a
/// later checkpoint.
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
        self.client().submit(make_request(payload, self.auth_token)?).await.map_err(DamlError::from)?;
        trace!(?command_id);
        Ok(command_id)
    }

    fn client(&self) -> CommandSubmissionServiceClient<Channel> {
        CommandSubmissionServiceClient::new(self.channel.clone())
    }
}
