use std::convert::TryFrom;
use std::fmt::Debug;

use futures::{Stream, StreamExt};
use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::filter::DamlEventFormat;
use crate::data::offset::DamlLedgerOffset;
use crate::data::state::{
    DamlActiveContractsPage, DamlActiveContractsResponse, DamlConnectedSynchronizer, DamlLatestPrunedOffsets,
};
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::state_service_client::StateServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{
    GetActiveContractsPageRequest, GetActiveContractsRequest, GetConnectedSynchronizersRequest, GetLatestPrunedOffsetsRequest,
    GetLedgerEndRequest,
};
use crate::service::common::make_request;

/// Snapshot- and topology-level queries against the participant's
/// state at a chosen offset.
///
/// v2's replacement for v1's ActiveContractsService (and absorbs
/// pieces of LedgerIdentityService / LedgerConfigurationService).
/// The typical "catch up then tail" client pattern uses
/// [`get_active_contracts`](Self::get_active_contracts) to seed
/// state, then switches to
/// [`UpdateService::get_updates`](crate::service::DamlUpdateService::get_updates)
/// from the snapshot's `active_at_offset`.
#[derive(Debug)]
pub struct DamlStateService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

impl<'a> DamlStateService<'a> {
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

    /// Stream a snapshot of the active contracts (plus incomplete
    /// reassignments) at `active_at_offset`.
    ///
    /// * `active_at_offset` — `BEGIN` (`0`) returns an empty set;
    ///   otherwise must be `<= ledger_end` and `>= last_pruning_offset`.
    /// * `event_format` — filters the result. Events take ACS-delta
    ///   shape regardless of any `transaction_shape` setting.
    /// * `stream_continuation_token` — pass a token from a prior
    ///   `GetActiveContractsResponse` to resume mid-snapshot. The
    ///   subsequent request must use the same `active_at_offset` and
    ///   `event_format`, and the participant must not have been
    ///   pruned past the active_at_offset in between.
    #[instrument(skip(self))]
    pub async fn get_active_contracts(
        &self,
        active_at_offset: impl Into<DamlLedgerOffset> + Debug,
        event_format: DamlEventFormat,
        stream_continuation_token: Option<Vec<u8>>,
    ) -> DamlResult<impl Stream<Item = DamlResult<DamlActiveContractsResponse>>> {
        let payload = GetActiveContractsRequest {
            active_at_offset: active_at_offset.into().value(),
            event_format: Some(event_format.into()),
            stream_continuation_token,
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let stream = self.client().get_active_contracts(make_request(payload, self.auth_token)?).await?.into_inner();
        Ok(stream.inspect(|r| trace!(?r)).map(|item| match item {
            Ok(response) => DamlActiveContractsResponse::try_from(response),
            Err(e) => Err(DamlError::from(e)),
        }))
    }

    /// Fetch one page of the active-contracts snapshot.
    ///
    /// `active_at_offset = None` defers to the participant — it picks
    /// its current ledger end and echoes the chosen offset in the
    /// response. Subsequent paging requests must echo whichever
    /// offset the first page returned, plus the same `event_format`,
    /// and must hit the same participant + Canton version.
    #[instrument(skip(self))]
    pub async fn get_active_contracts_page(
        &self,
        active_at_offset: Option<DamlLedgerOffset>,
        event_format: DamlEventFormat,
        max_page_size: Option<i32>,
        page_token: Option<Vec<u8>>,
    ) -> DamlResult<DamlActiveContractsPage> {
        let payload = GetActiveContractsPageRequest {
            active_at_offset: active_at_offset.map(DamlLedgerOffset::value),
            event_format: Some(event_format.into()),
            max_page_size,
            page_token,
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response =
            self.client().get_active_contracts_page(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlActiveContractsPage {
            active_contracts: response
                .active_contracts
                .into_iter()
                .map(DamlActiveContractsResponse::try_from)
                .collect::<DamlResult<_>>()?,
            active_at_offset: DamlLedgerOffset::new(response.active_at_offset),
            next_page_token: response.next_page_token,
        })
    }

    /// List the synchronizers the participant is currently connected
    /// to.
    ///
    /// `party` empty returns every synchronizer the participant is
    /// connected to; passing a specific party scopes the result and
    /// populates the per-synchronizer permission. `participant_id`
    /// empty queries the host participant.
    #[instrument(skip(self))]
    pub async fn get_connected_synchronizers(
        &self,
        party: impl Into<String> + Debug,
        participant_id: impl Into<String> + Debug,
        identity_provider_id: impl Into<String> + Debug,
    ) -> DamlResult<Vec<DamlConnectedSynchronizer>> {
        let payload = GetConnectedSynchronizersRequest {
            party: party.into(),
            participant_id: participant_id.into(),
            identity_provider_id: identity_provider_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response =
            self.client().get_connected_synchronizers(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        response.connected_synchronizers.into_iter().map(DamlConnectedSynchronizer::try_from).collect()
    }

    /// Current ledger end. Subscriptions started from this offset
    /// will only see events that landed *after* the call.
    ///
    /// `0` means the participant's view of the ledger is empty.
    #[instrument(skip(self))]
    pub async fn get_ledger_end(&self) -> DamlResult<DamlLedgerOffset> {
        let payload = GetLedgerEndRequest {};
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().get_ledger_end(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlLedgerOffset::new(response.offset))
    }

    /// Read the latest pruning watermarks. Both axes return `0`
    /// before any pruning has happened.
    #[instrument(skip(self))]
    pub async fn get_latest_pruned_offsets(&self) -> DamlResult<DamlLatestPrunedOffsets> {
        let payload = GetLatestPrunedOffsetsRequest {};
        trace!(payload = ?payload, token = ?self.auth_token);
        let response =
            self.client().get_latest_pruned_offsets(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlLatestPrunedOffsets {
            participant_pruned_up_to_inclusive: DamlLedgerOffset::new(response.participant_pruned_up_to_inclusive),
            all_divulged_contracts_pruned_up_to_inclusive: DamlLedgerOffset::new(
                response.all_divulged_contracts_pruned_up_to_inclusive,
            ),
        })
    }

    fn client(&self) -> StateServiceClient<Channel> {
        StateServiceClient::new(self.channel.clone())
    }
}
