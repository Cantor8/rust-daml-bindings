use std::fmt::Debug;

use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::offset::DamlLedgerOffset;
use crate::data::DamlResult;
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::participant_pruning_service_client::ParticipantPruningServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::PruneRequest;
use crate::service::common::make_request;

/// Truncate the oldest portion of the participant's ledger view in a way
/// that preserves the set of future allowed commands. Used both for
/// disk-footprint control and right-to-be-forgotten compliance.
pub struct DamlParticipantPruningService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

impl<'a> DamlParticipantPruningService<'a> {
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

    /// Prune everything up to and including `prune_up_to`. The call
    /// blocks until pruning completes (or errors out); on a busy
    /// participant this can take a while.
    ///
    /// Removes:
    ///   - normal and divulged contracts archived before `prune_up_to`,
    ///   - transaction events and completions before `prune_up_to`,
    ///   - immediately divulged contracts created before `prune_up_to`
    ///     (regardless of whether they were archived).
    ///
    /// `submission_id` is for logs only; empty string means "let the
    /// participant generate one".
    ///
    /// `prune_all_divulged_contracts` is preserved for v1 wire
    /// compatibility but is documented as a deprecated no-op in v2 —
    /// divulged contracts are pruned alongside the deactivated ones
    /// regardless of this flag.
    #[instrument(skip(self))]
    pub async fn prune(
        &self,
        prune_up_to: impl Into<DamlLedgerOffset> + Debug,
        submission_id: impl Into<String> + Debug,
        prune_all_divulged_contracts: bool,
    ) -> DamlResult<()> {
        let payload = PruneRequest {
            prune_up_to: prune_up_to.into().value(),
            submission_id: submission_id.into(),
            prune_all_divulged_contracts,
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        self.client().prune(make_request(payload, self.auth_token)?).await?;
        Ok(())
    }

    fn client(&self) -> ParticipantPruningServiceClient<Channel> {
        ParticipantPruningServiceClient::new(self.channel.clone())
    }
}
