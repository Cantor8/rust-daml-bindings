use std::convert::TryFrom;
use std::fmt::Debug;

use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::DamlResult;
use crate::data::event::DamlCreatedEvent;
use crate::grpc_protobuf::com::daml::ledger::api::v2::GetContractRequest;
use crate::grpc_protobuf::com::daml::ledger::api::v2::contract_service_client::ContractServiceClient;
use crate::service::common::make_request;
use crate::util::Required;

/// Look up contract data by contract id.
///
/// **Experimental / alpha.** No backward-compatibility guarantees per
/// the proto. Don't use against contracts that entered the
/// participant via party-replication or the repair service — the
/// participant rejects those.
///
/// Some fields of the returned [`DamlCreatedEvent`] are not populated
/// here (and should not be relied on): `offset`, `node_id`,
/// `created_event_blob`, `interface_views`, `acs_delta`. Use
/// [`DamlEventQueryService::get_events_by_contract_id`] or
/// [`DamlStateService::get_active_contracts`] when those fields
/// matter.
///
/// [`DamlEventQueryService::get_events_by_contract_id`]: crate::service::DamlEventQueryService::get_events_by_contract_id
/// [`DamlStateService::get_active_contracts`]: crate::service::DamlStateService::get_active_contracts
#[derive(Debug)]
pub struct DamlContractService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

impl<'a> DamlContractService<'a> {
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

    /// Fetch the contract identified by `contract_id`.
    ///
    /// `querying_parties` restricts the result to contracts whose
    /// stakeholder set intersects the listed parties; empty means
    /// "no party-based filter". The returned event's `witnesses`
    /// will contain only those `querying_parties` that are also
    /// stakeholders.
    ///
    /// Returns `CONTRACT_PAYLOAD_NOT_FOUND` if there's no such
    /// contract or no overlap with `querying_parties`.
    #[instrument(skip(self))]
    pub async fn get_contract(
        &self,
        contract_id: impl Into<String> + Debug,
        querying_parties: impl Into<Vec<String>> + Debug,
    ) -> DamlResult<DamlCreatedEvent> {
        let payload = GetContractRequest {
            contract_id: contract_id.into(),
            querying_parties: querying_parties.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().get_contract(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        DamlCreatedEvent::try_from(response.created_event.req()?)
    }

    fn client(&self) -> ContractServiceClient<Channel> {
        ContractServiceClient::new(self.channel.clone())
    }
}
