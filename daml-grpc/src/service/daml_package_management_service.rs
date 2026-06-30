use std::convert::TryFrom;
use std::fmt::Debug;

use bytes::Bytes;
use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::package::{
    DamlPackageDetails, DamlPriorTopologySerial, DamlUpdateVettedPackagesForceFlag, DamlUpdateVettedPackagesOutcome,
    DamlVettedPackages, DamlVettedPackagesChange, DamlVettingChange,
};
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::package_management_service_client::PackageManagementServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::upload_dar_file_request::VettingChange as ProtoVettingChange;
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::{
    ListKnownPackagesRequest, UpdateVettedPackagesRequest, UploadDarFileRequest, ValidateDarFileRequest,
    VettedPackagesChange,
};
use crate::service::common::make_request;

/// Inspect and manage the Daml-LF packages known to the participant.
///
/// v2 grew the surface significantly: DAR uploads now control whether
/// packages get vetted at upload time and on which synchronizer, plus
/// a new `ValidateDarFile` lets clients dry-run the upload-and-vet
/// checks, and `UpdateVettedPackages` exposes the vetting topology
/// directly to clients.
#[derive(Debug)]
pub struct DamlPackageManagementService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

impl<'a> DamlPackageManagementService<'a> {
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

    /// Returns the details of all Daml-LF packages known to the
    /// participant.
    #[instrument(skip(self))]
    pub async fn list_known_packages(&self) -> DamlResult<Vec<DamlPackageDetails>> {
        let payload = ListKnownPackagesRequest {};
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().list_known_packages(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        response.package_details.into_iter().map(DamlPackageDetails::try_from).collect()
    }

    /// Upload a DAR file.
    ///
    /// `vetting_change` controls whether the participant vets the
    /// contained packages immediately. When set to `VetAllPackages`,
    /// `synchronizer_id` names the synchronizer to vet on (required if
    /// the participant is connected to more than one synchronizer; on
    /// single-synchronizer participants it can be left empty). With
    /// `Unspecified` or `DontVetAnyPackages` the synchronizer field is
    /// ignored.
    ///
    /// Returns `UNIMPLEMENTED` if the participant doesn't support DAR
    /// uploads; `INVALID_ARGUMENT` if the DAR is too large or malformed.
    #[instrument(skip(self))]
    pub async fn upload_dar_file(
        &self,
        bytes: impl Into<Bytes> + Debug,
        submission_id: impl Into<String> + Debug,
        vetting_change: DamlVettingChange,
        synchronizer_id: impl Into<String> + Debug,
    ) -> DamlResult<()> {
        let payload = UploadDarFileRequest {
            dar_file: bytes.into().to_vec(),
            submission_id: submission_id.into(),
            vetting_change: ProtoVettingChange::from(vetting_change) as i32,
            synchronizer_id: synchronizer_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        self.client().upload_dar_file(make_request(payload, self.auth_token)?).await.map_err(DamlError::from)?;
        Ok(())
    }

    /// Run the same checks as [`upload_dar_file`](Self::upload_dar_file)
    /// without persisting the DAR or vetting anything. Useful for
    /// validating a DAR's upgrade compatibility before committing.
    #[instrument(skip(self))]
    pub async fn validate_dar_file(
        &self,
        bytes: impl Into<Bytes> + Debug,
        submission_id: impl Into<String> + Debug,
        synchronizer_id: impl Into<String> + Debug,
    ) -> DamlResult<()> {
        let payload = ValidateDarFileRequest {
            dar_file: bytes.into().to_vec(),
            submission_id: submission_id.into(),
            synchronizer_id: synchronizer_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        self.client().validate_dar_file(make_request(payload, self.auth_token)?).await.map_err(DamlError::from)?;
        Ok(())
    }

    /// Update the participant's vetting state on a synchronizer.
    ///
    /// Changes are applied in order; if any reference is unresolved or
    /// a `Vet` operation matches multiple packages, the whole request
    /// is rejected and the vetting state is unchanged.
    ///
    /// `expected_topology_serial` guards against concurrent updates —
    /// pass the serial you read most recently. Set to `None` to skip
    /// the check.
    ///
    /// `dry_run = true` performs the same validation but does not
    /// commit; pair with [`update_vetted_packages`](Self::update_vetted_packages)
    /// for a "preview, then apply" workflow.
    ///
    /// `force_flags` opt into vetting changes the server would
    /// otherwise reject for upgrade-safety reasons.
    #[instrument(skip(self, force_flags))]
    pub async fn update_vetted_packages(
        &self,
        changes: Vec<DamlVettedPackagesChange>,
        dry_run: bool,
        synchronizer_id: impl Into<String> + Debug,
        expected_topology_serial: Option<DamlPriorTopologySerial>,
        force_flags: impl IntoIterator<Item = DamlUpdateVettedPackagesForceFlag>,
    ) -> DamlResult<DamlUpdateVettedPackagesOutcome> {
        let payload = UpdateVettedPackagesRequest {
            changes: changes.into_iter().map(VettedPackagesChange::try_from).collect::<DamlResult<Vec<_>>>()?,
            dry_run,
            synchronizer_id: synchronizer_id.into(),
            expected_topology_serial: expected_topology_serial.map(Into::into),
            update_vetted_packages_force_flags: force_flags.into_iter().map(i32::from).collect(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response =
            self.client().update_vetted_packages(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlUpdateVettedPackagesOutcome {
            past_vetted_packages: response.past_vetted_packages.map(DamlVettedPackages::try_from).transpose()?,
            new_vetted_packages: response.new_vetted_packages.map(DamlVettedPackages::try_from).transpose()?,
        })
    }

    fn client(&self) -> PackageManagementServiceClient<Channel> {
        PackageManagementServiceClient::new(self.channel.clone())
    }
}
