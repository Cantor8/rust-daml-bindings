use std::time::Duration;

use crate::grpc_protobuf::com::daml::ledger::api::v2::{
    ExperimentalCommandInspectionService, ExperimentalFeatures, ExperimentalStaticTime, FeaturesDescriptor,
    OffsetCheckpointFeature, PackageFeature, PartyManagementFeature, UserManagementFeature,
};

/// Response of `VersionService.GetLedgerApiVersion`: the participant's
/// reported version string plus its feature descriptor. `features` is
/// wrapped in `Option` because non-compliant servers may omit it even
/// though the v2 spec marks it required.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlLedgerApiVersion {
    pub version: String,
    pub features: Option<DamlFeaturesDescriptor>,
}

/// The features advertised by a Ledger API v2 endpoint, as returned by
/// `VersionService.GetLedgerApiVersion`.
///
/// All inner fields are `Option`-wrapped because the Ledger API spec marks
/// them as required: their absence indicates a server that predates the
/// feature being reported. Clients can therefore use `Option::is_some` to
/// gate behavior on whether the participant knows about a feature at all.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlFeaturesDescriptor {
    pub experimental: Option<DamlExperimentalFeatures>,
    pub user_management: Option<DamlUserManagementFeature>,
    pub party_management: Option<DamlPartyManagementFeature>,
    pub offset_checkpoint: Option<DamlOffsetCheckpointFeature>,
    pub package: Option<DamlPackageFeature>,
}

impl From<FeaturesDescriptor> for DamlFeaturesDescriptor {
    fn from(f: FeaturesDescriptor) -> Self {
        Self {
            experimental: f.experimental.map(DamlExperimentalFeatures::from),
            user_management: f.user_management.map(DamlUserManagementFeature::from),
            party_management: f.party_management.map(DamlPartyManagementFeature::from),
            offset_checkpoint: f.offset_checkpoint.map(DamlOffsetCheckpointFeature::from),
            package: f.package_feature.map(DamlPackageFeature::from),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlExperimentalFeatures {
    pub static_time: Option<DamlExperimentalStaticTime>,
    pub command_inspection_service: Option<DamlExperimentalCommandInspectionService>,
}

impl From<ExperimentalFeatures> for DamlExperimentalFeatures {
    fn from(f: ExperimentalFeatures) -> Self {
        Self {
            static_time: f.static_time.map(DamlExperimentalStaticTime::from),
            command_inspection_service: f.command_inspection_service.map(DamlExperimentalCommandInspectionService::from),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlExperimentalStaticTime {
    pub supported: bool,
}

impl From<ExperimentalStaticTime> for DamlExperimentalStaticTime {
    fn from(f: ExperimentalStaticTime) -> Self {
        Self {
            supported: f.supported,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlExperimentalCommandInspectionService {
    pub supported: bool,
}

impl From<ExperimentalCommandInspectionService> for DamlExperimentalCommandInspectionService {
    fn from(f: ExperimentalCommandInspectionService) -> Self {
        Self {
            supported: f.supported,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlUserManagementFeature {
    pub supported: bool,
    /// `0` means the server enforces no per-user rights limit.
    pub max_rights_per_user: i32,
    /// `0` means the server enforces no page-size limit.
    pub max_users_page_size: i32,
}

impl From<UserManagementFeature> for DamlUserManagementFeature {
    fn from(f: UserManagementFeature) -> Self {
        Self {
            supported: f.supported,
            max_rights_per_user: f.max_rights_per_user,
            max_users_page_size: f.max_users_page_size,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlPartyManagementFeature {
    pub max_parties_page_size: i32,
}

impl From<PartyManagementFeature> for DamlPartyManagementFeature {
    fn from(f: PartyManagementFeature) -> Self {
        Self {
            max_parties_page_size: f.max_parties_page_size,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlPackageFeature {
    pub max_vetted_packages_page_size: i32,
}

impl From<PackageFeature> for DamlPackageFeature {
    fn from(f: PackageFeature) -> Self {
        Self {
            max_vetted_packages_page_size: f.max_vetted_packages_page_size,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlOffsetCheckpointFeature {
    pub max_offset_checkpoint_emission_delay: Duration,
}

impl From<OffsetCheckpointFeature> for DamlOffsetCheckpointFeature {
    fn from(f: OffsetCheckpointFeature) -> Self {
        Self {
            max_offset_checkpoint_emission_delay: f
                .max_offset_checkpoint_emission_delay
                .and_then(|d| Duration::try_from(d).ok())
                .unwrap_or_default(),
        }
    }
}
