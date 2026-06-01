use std::fmt::Debug;

use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::identity_provider::DamlIdentityProviderConfig;
use crate::data::DamlResult;
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::identity_provider_config_service_client::IdentityProviderConfigServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::{
    CreateIdentityProviderConfigRequest, DeleteIdentityProviderConfigRequest, GetIdentityProviderConfigRequest,
    ListIdentityProviderConfigsRequest, UpdateIdentityProviderConfigRequest,
};
use crate::service::common::make_request;
use crate::util::Required;

/// Manage runtime-configured Identity Providers on the participant.
///
/// The default IDP is fixed at deployment; this service lets admins
/// add, modify, and remove additional IDPs at runtime. Each IDP scopes
/// its own users and parties (each carrying that IDP's id), so an IDP
/// admin can manage their own slice without touching others.
///
/// Required authorization: `HasRight(ParticipantAdmin)` — only
/// participant admins can manage IDP configurations.
#[derive(Debug)]
pub struct DamlIdentityProviderConfigService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

impl<'a> DamlIdentityProviderConfigService<'a> {
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

    /// Create a new identity-provider configuration. Fails if the
    /// participant's per-deployment IDP cap has been reached.
    #[instrument(skip(self))]
    pub async fn create_identity_provider_config(
        &self,
        config: DamlIdentityProviderConfig,
    ) -> DamlResult<DamlIdentityProviderConfig> {
        let payload = CreateIdentityProviderConfigRequest {
            identity_provider_config: Some(config.into()),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response =
            self.client().create_identity_provider_config(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlIdentityProviderConfig::from(response.identity_provider_config.req()?))
    }

    /// Fetch an IDP configuration by id.
    #[instrument(skip(self))]
    pub async fn get_identity_provider_config(
        &self,
        identity_provider_id: impl Into<String> + Debug,
    ) -> DamlResult<DamlIdentityProviderConfig> {
        let payload = GetIdentityProviderConfigRequest {
            identity_provider_id: identity_provider_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response =
            self.client().get_identity_provider_config(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlIdentityProviderConfig::from(response.identity_provider_config.req()?))
    }

    /// Update modifiable fields of an IDP configuration. The
    /// `identity_provider_id` field of `config` identifies which IDP to
    /// touch; only paths listed in `update_paths` are written.
    /// Modifiable paths are `is_deactivated`, `issuer`, `jwks_url`, and
    /// `audience`.
    #[instrument(skip(self, update_paths))]
    pub async fn update_identity_provider_config(
        &self,
        config: DamlIdentityProviderConfig,
        update_paths: impl IntoIterator<Item = String>,
    ) -> DamlResult<DamlIdentityProviderConfig> {
        let payload = UpdateIdentityProviderConfigRequest {
            identity_provider_config: Some(config.into()),
            update_mask: Some(prost_types::FieldMask {
                paths: update_paths.into_iter().collect(),
            }),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response =
            self.client().update_identity_provider_config(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlIdentityProviderConfig::from(response.identity_provider_config.req()?))
    }

    /// List every IDP configuration. The proto explicitly notes the
    /// result set is bounded and pagination is unnecessary.
    #[instrument(skip(self))]
    pub async fn list_identity_provider_configs(&self) -> DamlResult<Vec<DamlIdentityProviderConfig>> {
        let payload = ListIdentityProviderConfigsRequest {};
        trace!(payload = ?payload, token = ?self.auth_token);
        let response =
            self.client().list_identity_provider_configs(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(response.identity_provider_configs.into_iter().map(DamlIdentityProviderConfig::from).collect())
    }

    /// Delete an IDP configuration. Users and parties still scoped to
    /// the deleted IDP become inaccessible until reassigned via
    /// `UpdatePartyIdentityProviderId` / `UpdateUserIdentityProviderId`.
    #[instrument(skip(self))]
    pub async fn delete_identity_provider_config(
        &self,
        identity_provider_id: impl Into<String> + Debug,
    ) -> DamlResult<()> {
        let payload = DeleteIdentityProviderConfigRequest {
            identity_provider_id: identity_provider_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        self.client().delete_identity_provider_config(make_request(payload, self.auth_token)?).await?;
        Ok(())
    }

    fn client(&self) -> IdentityProviderConfigServiceClient<Channel> {
        IdentityProviderConfigServiceClient::new(self.channel.clone())
    }
}
