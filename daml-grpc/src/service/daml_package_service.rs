use std::convert::TryFrom;
use std::fmt::Debug;

use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::DamlResult;
use crate::data::package::{
    DamlPackage, DamlPackageMetadataFilter, DamlPackageStatus, DamlTopologyStateFilter, DamlVettedPackages,
    DamlVettedPackagesPage,
};
use crate::grpc_protobuf::com::daml::ledger::api::v2::package_service_client::PackageServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{
    GetPackageRequest, GetPackageStatusRequest, ListPackagesRequest, ListVettedPackagesRequest, PackageStatus,
};
use crate::service::common::make_request;
use crate::util::Required;

/// Query and extract the Daml-LF packages that are supported by a v2
/// participant. Also exposes the `ListVettedPackages` RPC introduced in v2,
/// which surfaces topology-level vetting state.
#[derive(Debug)]
pub struct DamlPackageService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

impl<'a> DamlPackageService<'a> {
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

    /// Return every package id supported by the participant.
    #[instrument(skip(self))]
    pub async fn list_packages(&self) -> DamlResult<Vec<String>> {
        let payload = ListPackagesRequest {};
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().list_packages(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(response.package_ids)
    }

    /// Fetch the on-wire contents of a single package by id. The returned
    /// payload is a `daml_lf.ArchivePayload`.
    #[instrument(skip(self))]
    pub async fn get_package(&self, package_id: impl Into<String> + Debug) -> DamlResult<DamlPackage> {
        let payload = GetPackageRequest {
            package_id: package_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().get_package(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        DamlPackage::try_from(response)
    }

    /// Check whether the participant has registered (and is willing to
    /// interpret commands referencing) a given package.
    #[instrument(skip(self))]
    pub async fn get_package_status(&self, package_id: impl Into<String> + Debug) -> DamlResult<DamlPackageStatus> {
        let payload = GetPackageStatusRequest {
            package_id: package_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().get_package_status(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlPackageStatus::from(PackageStatus::try_from(response.package_status).ok().req()?))
    }

    /// List packages vetted on the network, optionally filtered by package
    /// metadata and / or topology pair.
    ///
    /// Pagination: pass an empty `page_token` for the first request; the
    /// next call should use `DamlVettedPackagesPage::next_page_token` until
    /// it comes back empty. `page_size = 0` lets the server pick a
    /// reasonable default; the server's hard cap is advertised in the
    /// `package_feature` field of `VersionService.GetLedgerApiVersion`.
    #[instrument(skip(self))]
    pub async fn list_vetted_packages(
        &self,
        package_metadata_filter: Option<DamlPackageMetadataFilter>,
        topology_state_filter: Option<DamlTopologyStateFilter>,
        page_token: impl Into<String> + Debug,
        page_size: u32,
    ) -> DamlResult<DamlVettedPackagesPage> {
        let payload = ListVettedPackagesRequest {
            package_metadata_filter: package_metadata_filter.map(Into::into),
            topology_state_filter: topology_state_filter.map(Into::into),
            page_token: page_token.into(),
            page_size,
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().list_vetted_packages(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlVettedPackagesPage {
            vetted_packages: response
                .vetted_packages
                .into_iter()
                .map(DamlVettedPackages::try_from)
                .collect::<DamlResult<Vec<_>>>()?,
            next_page_token: response.next_page_token,
        })
    }

    fn client(&self) -> PackageServiceClient<Channel> {
        PackageServiceClient::new(self.channel.clone())
    }
}
