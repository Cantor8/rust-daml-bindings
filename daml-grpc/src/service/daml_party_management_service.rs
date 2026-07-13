use std::fmt::Debug;

use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::DamlResult;
use crate::data::party::{DamlObjectMeta, DamlPartyDetails};
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::party_management_service_client::PartyManagementServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::{
    AllocatePartyRequest, GetParticipantIdRequest, GetPartiesRequest, ListKnownPartiesRequest,
    UpdatePartyDetailsRequest, UpdatePartyIdentityProviderIdRequest,
};
use crate::service::common::make_request;
use crate::util::Required;

/// Inspect and manage the party state of the v2 participant.
///
/// v2 grew the surface: pagination on `ListKnownParties`, identity-
/// provider scoping on every read RPC, `UpdatePartyDetails` (with
/// FieldMask-driven partial updates), and party-to-IDP reassignment.
///
/// Not yet exposed in this checkpoint:
/// `AllocateExternalParty` and `GenerateExternalPartyTopology`. They
/// require the v2 cryptographic types (`SigningPublicKey`, `Signature`)
/// and `TopologyTransaction` wrappers, which belong to a later checkpoint.
///
/// # Authorization
///
/// When the participant requires authentication, all RPCs respond with
/// `UNAUTHENTICATED` if a valid access token is missing, and with
/// `PERMISSION_DENIED` if the token's claims are insufficient.
#[derive(Debug)]
pub struct DamlPartyManagementService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

/// Listing page returned by [`DamlPartyManagementService::list_known_parties`].
/// Use `next_page_token` as the input cursor for the next call; an empty
/// token marks the last page.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlPartyDetailsPage {
    pub party_details: Vec<DamlPartyDetails>,
    pub next_page_token: String,
}

impl<'a> DamlPartyManagementService<'a> {
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

    /// Returns the identifier of the participant. Horizontally-scaled
    /// replicas return the same id. Always succeeds when the participant
    /// is healthy.
    #[instrument(skip(self))]
    pub async fn get_participant_id(&self) -> DamlResult<String> {
        let payload = GetParticipantIdRequest {};
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().get_participant_id(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(response.participant_id)
    }

    /// Get details for a fixed set of parties. Unknown parties are
    /// silently omitted from the response — match against `party` to
    /// detect gaps. Result order is not guaranteed to mirror the input.
    ///
    /// `identity_provider_id`: empty means "default IDP / not hosted
    /// locally"; the request is scoped to the named IDP otherwise.
    #[instrument(skip(self))]
    pub async fn get_parties(
        &self,
        parties: impl Into<Vec<String>> + Debug,
        identity_provider_id: impl Into<String> + Debug,
    ) -> DamlResult<Vec<DamlPartyDetails>> {
        let payload = GetPartiesRequest {
            parties: parties.into(),
            identity_provider_id: identity_provider_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().get_parties(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(response.party_details.into_iter().map(DamlPartyDetails::from).collect())
    }

    /// List parties known to the participant.
    ///
    /// `page_size = 0` lets the server pick; the server's hard cap is in
    /// `PartyManagementFeature::max_parties_page_size` (returned by
    /// `VersionService.GetLedgerApiVersion`). Pass an empty
    /// `page_token` to start; loop until the returned token is empty.
    ///
    /// `filter_party` does a prefix match against the party id; empty
    /// means no filter.
    #[instrument(skip(self))]
    pub async fn list_known_parties(
        &self,
        identity_provider_id: impl Into<String> + Debug,
        page_token: impl Into<String> + Debug,
        page_size: i32,
        filter_party: impl Into<String> + Debug,
    ) -> DamlResult<DamlPartyDetailsPage> {
        let payload = ListKnownPartiesRequest {
            page_token: page_token.into(),
            page_size,
            identity_provider_id: identity_provider_id.into(),
            filter_party: filter_party.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().list_known_parties(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlPartyDetailsPage {
            party_details: response.party_details.into_iter().map(DamlPartyDetails::from).collect(),
            next_page_token: response.next_page_token,
        })
    }

    /// Allocate a new participant-local party.
    ///
    /// All inputs are optional, including `party_id_hint` (the
    /// participant may invent a fresh id even if you supply a hint).
    /// `synchronizer_id` is only required when the participant is
    /// connected to more than one synchronizer.
    ///
    /// `user_id`, when set, gives the named user `act_as` rights on the
    /// freshly allocated party.
    #[instrument(skip(self))]
    pub async fn allocate_party(
        &self,
        party_id_hint: impl Into<String> + Debug,
        local_metadata: Option<DamlObjectMeta>,
        identity_provider_id: impl Into<String> + Debug,
        synchronizer_id: impl Into<String> + Debug,
        user_id: impl Into<String> + Debug,
    ) -> DamlResult<DamlPartyDetails> {
        let payload = AllocatePartyRequest {
            party_id_hint: party_id_hint.into(),
            local_metadata: local_metadata.map(Into::into),
            identity_provider_id: identity_provider_id.into(),
            synchronizer_id: synchronizer_id.into(),
            user_id: user_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().allocate_party(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlPartyDetails::from(response.party_details.req()?))
    }

    /// Update modifiable fields of an existing `PartyDetails`.
    ///
    /// `update_paths` is the `FieldMask`: only the paths you name are
    /// touched. Modifiable paths today are limited to `local_metadata`
    /// and its sub-paths (e.g. `local_metadata.annotations`). The other
    /// fields (`party`, `is_local`, `local_metadata.resource_version`)
    /// may also appear in `update_paths`, but only to identify the
    /// resource or assert optimistic-concurrency invariants — their
    /// values in `party_details` must match the server's.
    #[instrument(skip(self, update_paths))]
    pub async fn update_party_details(
        &self,
        party_details: DamlPartyDetails,
        update_paths: impl IntoIterator<Item = String>,
    ) -> DamlResult<DamlPartyDetails> {
        let payload = UpdatePartyDetailsRequest {
            party_details: Some(party_details.into()),
            update_mask: Some(prost_types::FieldMask {
                paths: update_paths.into_iter().collect(),
            }),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().update_party_details(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlPartyDetails::from(response.party_details.req()?))
    }

    /// Reassign a party to a different `Identity Provider`. Pass the
    /// current and target IDP ids (use empty string for the default
    /// IDP).
    #[instrument(skip(self))]
    pub async fn update_party_identity_provider_id(
        &self,
        party: impl Into<String> + Debug,
        source_identity_provider_id: impl Into<String> + Debug,
        target_identity_provider_id: impl Into<String> + Debug,
    ) -> DamlResult<()> {
        let payload = UpdatePartyIdentityProviderIdRequest {
            party: party.into(),
            source_identity_provider_id: source_identity_provider_id.into(),
            target_identity_provider_id: target_identity_provider_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        self.client().update_party_identity_provider_id(make_request(payload, self.auth_token)?).await?;
        Ok(())
    }

    fn client(&self) -> PartyManagementServiceClient<Channel> {
        PartyManagementServiceClient::new(self.channel.clone())
    }
}
