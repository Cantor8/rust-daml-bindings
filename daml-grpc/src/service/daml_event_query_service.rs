use std::convert::TryFrom;
use std::fmt::Debug;

use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::DamlResult;
use crate::data::event_query::DamlEventsByContractId;
use crate::data::filter::DamlEventFormat;
use crate::grpc_protobuf::com::daml::ledger::api::v2::GetEventsByContractIdRequest;
use crate::grpc_protobuf::com::daml::ledger::api::v2::event_query_service_client::EventQueryServiceClient;
use crate::service::common::make_request;

/// Look up the create + consuming-archive events for a single
/// contract by id. Distinct from streaming history because the
/// participant returns just the two endpoint events, not the full
/// transaction(s) they appeared in.
///
/// Contract-key lookup is **not** supported in v2; multi-synchronizer
/// participants don't (yet) have a globally unique contract-key
/// notion. Use `get_events_by_contract_id` when the contract id is
/// in hand.
#[derive(Debug)]
pub struct DamlEventQueryService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

impl<'a> DamlEventQueryService<'a> {
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

    /// Fetch the create and archive events for `contract_id`.
    ///
    /// Events are filtered through `event_format`; results take
    /// ACS-delta shape regardless of any `transaction_shape` setting.
    /// Returns `CONTRACT_EVENTS_NOT_FOUND` when the contract is
    /// unknown to the participant or every matching event has been
    /// pruned.
    #[instrument(skip(self))]
    pub async fn get_events_by_contract_id(
        &self,
        contract_id: impl Into<String> + Debug,
        event_format: DamlEventFormat,
    ) -> DamlResult<DamlEventsByContractId> {
        let payload = GetEventsByContractIdRequest {
            contract_id: contract_id.into(),
            event_format: Some(event_format.into()),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response =
            self.client().get_events_by_contract_id(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        DamlEventsByContractId::try_from(response)
    }

    fn client(&self) -> EventQueryServiceClient<Channel> {
        EventQueryServiceClient::new(self.channel.clone())
    }
}
