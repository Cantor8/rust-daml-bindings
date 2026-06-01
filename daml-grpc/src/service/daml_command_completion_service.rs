use std::convert::TryFrom;
use std::fmt::Debug;

use futures::{Stream, StreamExt};
use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::completion::DamlCompletionResponse;
use crate::data::offset::DamlLedgerOffset;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::command_completion_service_client::CommandCompletionServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::CompletionStreamRequest;
use crate::service::common::make_request;

/// Observe the status of command submissions on a v2 participant.
///
/// v2 collapsed the v1 service: there is no longer a `CompletionEnd`
/// RPC — use `StateService.GetLedgerEnd` to fetch the current ledger
/// end offset.
#[derive(Debug)]
pub struct DamlCommandCompletionService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

impl<'a> DamlCommandCompletionService<'a> {
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

    /// Subscribe to the completion stream.
    ///
    /// * `user_id` — only completions from this user are returned. Ignored
    ///   when the request is authenticated with a user token (the
    ///   token's user-id wins).
    /// * `parties` — non-empty; a completion is included if any of its
    ///   `act_as` parties appear in this set.
    /// * `begin_exclusive` — minimum offset to resume from (`BEGIN` for
    ///   "from the start"). Must be greater than the ledger's prune
    ///   offset if pruning has happened.
    ///
    /// The returned stream yields one [`DamlCompletionResponse`] per
    /// message — either a [`Completion`](DamlCompletionResponse::Completion)
    /// or an [`OffsetCheckpoint`](DamlCompletionResponse::OffsetCheckpoint).
    #[instrument(skip(self))]
    pub async fn get_completion_stream(
        &self,
        user_id: impl Into<String> + Debug,
        parties: impl Into<Vec<String>> + Debug,
        begin_exclusive: impl Into<DamlLedgerOffset> + Debug,
    ) -> DamlResult<impl Stream<Item = DamlResult<DamlCompletionResponse>>> {
        let payload = CompletionStreamRequest {
            user_id: user_id.into(),
            parties: parties.into(),
            begin_exclusive: begin_exclusive.into().value(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let completion_stream =
            self.client().completion_stream(make_request(payload, self.auth_token)?).await?.into_inner();
        Ok(completion_stream.inspect(|response| trace!(?response)).map(|item| match item {
            Ok(completion) => DamlCompletionResponse::try_from(completion),
            Err(e) => Err(DamlError::from(e)),
        }))
    }

    fn client(&self) -> CommandCompletionServiceClient<Channel> {
        CommandCompletionServiceClient::new(self.channel.clone())
    }
}
