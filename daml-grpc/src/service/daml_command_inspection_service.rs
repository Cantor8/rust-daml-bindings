use std::convert::TryFrom;
use std::fmt::Debug;

use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::inspection::{DamlCommandState, DamlCommandStatus};
use crate::data::DamlResult;
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::command_inspection_service_client::CommandInspectionServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::{CommandState, GetCommandStatusRequest};
use crate::service::common::make_request;

/// Inspect the in-memory status of commands the participant is
/// currently tracking.
///
/// v2 introduces this service for *debugging only* — the participant
/// tracks status entries in memory (no persistence), the API is alpha,
/// and the proto explicitly says backward compatibility is not
/// guaranteed. Don't build production paths on top of it. Whether it
/// is even exposed by a given participant is signalled by the
/// `experimental.command_inspection_service.supported` feature flag
/// returned by `VersionService.GetLedgerApiVersion`.
///
/// Requires participant admin authorization.
#[derive(Debug)]
pub struct DamlCommandInspectionService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

impl<'a> DamlCommandInspectionService<'a> {
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

    /// Query the participant for command status entries.
    ///
    /// * `command_id_prefix` — empty matches any command id.
    /// * `state` — pass `DamlCommandState::Unspecified` as a wildcard
    ///   ("match any state"); otherwise filters by lifecycle state.
    /// * `limit` — defaults to 100 server-side when `0`.
    #[instrument(skip(self))]
    pub async fn get_command_status(
        &self,
        command_id_prefix: impl Into<String> + Debug,
        state: DamlCommandState,
        limit: u32,
    ) -> DamlResult<Vec<DamlCommandStatus>> {
        let payload = GetCommandStatusRequest {
            command_id_prefix: command_id_prefix.into(),
            state: CommandState::from(state) as i32,
            limit,
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().get_command_status(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        response.command_status.into_iter().map(DamlCommandStatus::try_from).collect()
    }

    fn client(&self) -> CommandInspectionServiceClient<Channel> {
        CommandInspectionServiceClient::new(self.channel.clone())
    }
}
