use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::{DamlFeaturesDescriptor, DamlLedgerApiVersion, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::version_service_client::VersionServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::GetLedgerApiVersionRequest;
use crate::service::common::make_request;

/// Retrieve information about the Ledger API version.
///
/// The v2 request carries no fields — participants no longer scope responses
/// by ledger-id — so this service only needs a channel and an optional auth
/// token.
#[derive(Debug)]
pub struct DamlVersionService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

impl<'a> DamlVersionService<'a> {
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

    /// Read the Ledger API version.
    #[instrument(skip(self))]
    pub async fn get_ledger_api_version(&self) -> DamlResult<DamlLedgerApiVersion> {
        let payload = GetLedgerApiVersionRequest {};
        trace!(payload = ?payload, token = ?self.auth_token);
        let response =
            self.client().get_ledger_api_version(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlLedgerApiVersion {
            version: response.version,
            features: response.features.map(DamlFeaturesDescriptor::from),
        })
    }

    fn client(&self) -> VersionServiceClient<Channel> {
        VersionServiceClient::new(self.channel.clone())
    }
}
