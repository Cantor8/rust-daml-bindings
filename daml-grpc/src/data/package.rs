use std::convert::TryFrom;

use chrono::{DateTime, Utc};

use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::upload_dar_file_request::VettingChange;
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::vetted_packages_change::{Operation, Unvet, Vet};
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::{
    PackageDetails, UpdateVettedPackagesForceFlag, VettedPackagesChange, VettedPackagesRef,
};
use crate::grpc_protobuf::com::daml::ledger::api::v2::prior_topology_serial::Serial;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{
    GetPackageResponse, HashFunction, PackageMetadataFilter, PackageReference, PackageStatus, PriorTopologySerial,
    TopologyStateFilter, VettedPackage, VettedPackages,
};
use crate::util;
use crate::util::Required;

/// The contents of a Daml-LF package returned by `PackageService.GetPackage`.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlPackage {
    payload: Vec<u8>,
    hash: String,
    hash_function: DamlHashFunction,
}

impl DamlPackage {
    pub fn new(
        payload: impl Into<Vec<u8>>,
        hash: impl Into<String>,
        hash_function: impl Into<DamlHashFunction>,
    ) -> Self {
        Self {
            payload: payload.into(),
            hash: hash.into(),
            hash_function: hash_function.into(),
        }
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn take_payload(self) -> Vec<u8> {
        self.payload
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub const fn hash_function(&self) -> &DamlHashFunction {
        &self.hash_function
    }
}

impl TryFrom<GetPackageResponse> for DamlPackage {
    type Error = DamlError;

    fn try_from(response: GetPackageResponse) -> DamlResult<Self> {
        Ok(Self::new(response.archive_payload, response.hash, HashFunction::try_from(response.hash_function).ok().req()?))
    }
}

/// Whether the participant has registered a given Daml-LF package and is
/// willing to interpret commands that reference it.
///
/// v2 renamed v1's `PACKAGE_STATUS_UNKNOWN` to `PACKAGE_STATUS_UNSPECIFIED`;
/// the semantics ("the participant doesn't know about this package") are
/// unchanged.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlPackageStatus {
    Unspecified,
    Registered,
}

impl From<PackageStatus> for DamlPackageStatus {
    fn from(status: PackageStatus) -> Self {
        match status {
            PackageStatus::Unspecified => DamlPackageStatus::Unspecified,
            PackageStatus::Registered => DamlPackageStatus::Registered,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlHashFunction {
    Sha256,
}

impl From<HashFunction> for DamlHashFunction {
    fn from(hash_function: HashFunction) -> Self {
        match hash_function {
            HashFunction::Sha256 => DamlHashFunction::Sha256,
        }
    }
}

/// Detailed information about a Daml DAR package, as reported by
/// `PackageManagementService.ListKnownPackages`.
///
/// v2 dropped `source_description` and added `name` and `version`, which are
/// drawn from the package metadata inside the DAR.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlPackageDetails {
    pub package_id: String,
    pub package_size: u64,
    pub known_since: DateTime<Utc>,
    pub name: String,
    pub version: String,
}

impl TryFrom<PackageDetails> for DamlPackageDetails {
    type Error = DamlError;

    fn try_from(details: PackageDetails) -> DamlResult<Self> {
        Ok(Self {
            package_id: details.package_id,
            package_size: details.package_size,
            known_since: util::from_grpc_timestamp(&details.known_since.req()?)?,
            name: details.name,
            version: details.version,
        })
    }
}

/// A reference to a Daml-LF package by id + name + version. Returned in v2
/// alongside many event/contract messages so clients can resolve interface
/// implementations and version constraints without a separate lookup.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlPackageReference {
    pub package_id: String,
    pub package_name: String,
    pub package_version: String,
}

impl From<PackageReference> for DamlPackageReference {
    fn from(r: PackageReference) -> Self {
        Self {
            package_id: r.package_id,
            package_name: r.package_name,
            package_version: r.package_version,
        }
    }
}

/// A single vetted package as reported by
/// `PackageService.ListVettedPackages`. Vetting bounds (`valid_from_inclusive`,
/// `valid_until_exclusive`) are open intervals when `None`.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlVettedPackage {
    pub package_id: String,
    pub valid_from_inclusive: Option<DateTime<Utc>>,
    pub valid_until_exclusive: Option<DateTime<Utc>>,
    pub package_name: Option<String>,
    pub package_version: Option<String>,
}

impl TryFrom<VettedPackage> for DamlVettedPackage {
    type Error = DamlError;

    fn try_from(p: VettedPackage) -> DamlResult<Self> {
        Ok(Self {
            package_id: p.package_id,
            valid_from_inclusive: p.valid_from_inclusive.as_ref().map(util::from_grpc_timestamp).transpose()?,
            valid_until_exclusive: p.valid_until_exclusive.as_ref().map(util::from_grpc_timestamp).transpose()?,
            // The proto marks these as required-when-present at the participant.
            // We surface "" as None so callers don't have to disambiguate empty
            // strings from "field actually missing".
            package_name: Some(p.package_name).filter(|s| !s.is_empty()),
            package_version: Some(p.package_version).filter(|s| !s.is_empty()),
        })
    }
}

/// A list of packages vetted on a given participant + synchronizer pair.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlVettedPackages {
    pub packages: Vec<DamlVettedPackage>,
    pub participant_id: String,
    pub synchronizer_id: String,
    pub topology_serial: u32,
}

impl TryFrom<VettedPackages> for DamlVettedPackages {
    type Error = DamlError;

    fn try_from(v: VettedPackages) -> DamlResult<Self> {
        Ok(Self {
            packages: v
                .packages
                .into_iter()
                .map(DamlVettedPackage::try_from)
                .collect::<DamlResult<Vec<_>>>()?,
            participant_id: v.participant_id,
            synchronizer_id: v.synchronizer_id,
            topology_serial: v.topology_serial,
        })
    }
}

/// Filter for `ListVettedPackages` by package metadata.
///
/// Both fields are OR-combined: a package matches if its id appears in
/// `package_ids` *or* its name starts with one of `package_name_prefixes`.
/// An empty filter matches every vetted package.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlPackageMetadataFilter {
    pub package_ids: Vec<String>,
    pub package_name_prefixes: Vec<String>,
}

impl From<DamlPackageMetadataFilter> for PackageMetadataFilter {
    fn from(f: DamlPackageMetadataFilter) -> Self {
        Self {
            package_ids: f.package_ids,
            package_name_prefixes: f.package_name_prefixes,
        }
    }
}

/// Filter for `ListVettedPackages` by participant + synchronizer topology.
///
/// Both fields are AND-combined when set: a package matches only if it is
/// hosted on one of `participant_ids` *and* vetted on one of
/// `synchronizer_ids`. An empty filter matches every topology pair.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlTopologyStateFilter {
    pub participant_ids: Vec<String>,
    pub synchronizer_ids: Vec<String>,
}

impl From<DamlTopologyStateFilter> for TopologyStateFilter {
    fn from(f: DamlTopologyStateFilter) -> Self {
        Self {
            participant_ids: f.participant_ids,
            synchronizer_ids: f.synchronizer_ids,
        }
    }
}

/// A single page of `ListVettedPackages` results. Carries the cursor
/// required to fetch the next page; an empty `next_page_token` means there
/// are no more pages.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlVettedPackagesPage {
    pub vetted_packages: Vec<DamlVettedPackages>,
    pub next_page_token: String,
}

// ---------------------------------------------------------------------------
// PackageManagementService: DAR upload + vetting administration
// ---------------------------------------------------------------------------

/// How the participant should treat packages contained in an uploaded DAR
/// w.r.t. vetting. `Unspecified` defers to the server default, which is
/// "vet everything".
#[derive(Debug, Eq, PartialEq, Clone, Copy, Default)]
pub enum DamlVettingChange {
    #[default]
    Unspecified,
    VetAllPackages,
    DontVetAnyPackages,
}

impl From<DamlVettingChange> for VettingChange {
    fn from(v: DamlVettingChange) -> Self {
        match v {
            DamlVettingChange::Unspecified => VettingChange::Unspecified,
            DamlVettingChange::VetAllPackages => VettingChange::VetAllPackages,
            DamlVettingChange::DontVetAnyPackages => VettingChange::DontVetAnyPackages,
        }
    }
}

/// A reference matching one or more vetted packages. At least one of
/// `package_id` or `package_name` must be set; empty fields act as
/// wildcards. `Vet` operations require unique matches; `Unvet`
/// operations may match multiple.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlVettedPackagesRef {
    pub package_id: String,
    pub package_name: String,
    pub package_version: String,
}

impl From<DamlVettedPackagesRef> for VettedPackagesRef {
    fn from(r: DamlVettedPackagesRef) -> Self {
        Self {
            package_id: r.package_id,
            package_name: r.package_name,
            package_version: r.package_version,
        }
    }
}

/// One step in an `UpdateVettedPackages` request. `Vet` adds or extends
/// vetting bounds; `Unvet` removes packages from the vetted set. Changes
/// are applied in order and either all succeed or all fail.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlVettedPackagesChange {
    Vet {
        packages: Vec<DamlVettedPackagesRef>,
        /// `None` removes the lower bound (vetted from the beginning of
        /// time); `Some(t)` overwrites any prior lower bound.
        new_valid_from_inclusive: Option<DateTime<Utc>>,
        /// `None` removes the upper bound (vetted indefinitely);
        /// `Some(t)` overwrites any prior upper bound.
        new_valid_until_exclusive: Option<DateTime<Utc>>,
    },
    Unvet {
        packages: Vec<DamlVettedPackagesRef>,
    },
}

impl TryFrom<DamlVettedPackagesChange> for VettedPackagesChange {
    type Error = DamlError;

    fn try_from(c: DamlVettedPackagesChange) -> DamlResult<Self> {
        let op = match c {
            DamlVettedPackagesChange::Vet {
                packages,
                new_valid_from_inclusive,
                new_valid_until_exclusive,
            } => Operation::Vet(Vet {
                packages: packages.into_iter().map(Into::into).collect(),
                new_valid_from_inclusive: new_valid_from_inclusive.map(util::to_grpc_timestamp).transpose()?,
                new_valid_until_exclusive: new_valid_until_exclusive.map(util::to_grpc_timestamp).transpose()?,
            }),
            DamlVettedPackagesChange::Unvet {
                packages,
            } => Operation::Unvet(Unvet {
                packages: packages.into_iter().map(Into::into).collect(),
            }),
        };
        Ok(Self {
            operation: Some(op),
        })
    }
}

/// Concurrency-control token for `UpdateVettedPackages`. The participant
/// rejects the update if its current topology-transaction serial doesn't
/// match the supplied value.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum DamlPriorTopologySerial {
    /// The participant must currently be at this serial.
    Prior(u32),
    /// The participant must have no prior vetting transactions on this
    /// (participant, synchronizer) pair.
    NoPrior,
}

impl From<DamlPriorTopologySerial> for PriorTopologySerial {
    fn from(s: DamlPriorTopologySerial) -> Self {
        let serial = match s {
            DamlPriorTopologySerial::Prior(v) => Serial::Prior(v),
            DamlPriorTopologySerial::NoPrior => Serial::NoPrior(()),
        };
        Self {
            serial: Some(serial),
        }
    }
}

/// Opt-in escapes for vetting updates that are normally rejected because
/// they would compromise upgrade safety. Use sparingly.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum DamlUpdateVettedPackagesForceFlag {
    AllowVetIncompatibleUpgrades,
    AllowUnvettedDependencies,
}

impl From<DamlUpdateVettedPackagesForceFlag> for i32 {
    fn from(f: DamlUpdateVettedPackagesForceFlag) -> Self {
        let v: UpdateVettedPackagesForceFlag = f.into();
        v as i32
    }
}

impl From<DamlUpdateVettedPackagesForceFlag> for UpdateVettedPackagesForceFlag {
    fn from(f: DamlUpdateVettedPackagesForceFlag) -> Self {
        match f {
            DamlUpdateVettedPackagesForceFlag::AllowVetIncompatibleUpgrades =>
                UpdateVettedPackagesForceFlag::AllowVetIncompatibleUpgrades,
            DamlUpdateVettedPackagesForceFlag::AllowUnvettedDependencies =>
                UpdateVettedPackagesForceFlag::AllowUnvettedDependencies,
        }
    }
}

/// Outcome of `UpdateVettedPackages`. `past_vetted_packages` is `None`
/// when no prior vetting topology existed for this participant on the
/// target synchronizer.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlUpdateVettedPackagesOutcome {
    pub past_vetted_packages: Option<DamlVettedPackages>,
    pub new_vetted_packages: Option<DamlVettedPackages>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each DamlVettingChange must map to a distinct proto enum
    /// value. Collisions would silently downgrade a vet operation
    /// (e.g. `VetAllPackages` -> `Unspecified` would defer to the
    /// server default unintentionally).
    #[test]
    fn vetting_change_variants_are_injective() {
        let unspecified: VettingChange = DamlVettingChange::Unspecified.into();
        let vet: VettingChange = DamlVettingChange::VetAllPackages.into();
        let dont_vet: VettingChange = DamlVettingChange::DontVetAnyPackages.into();
        assert_ne!(unspecified as i32, vet as i32);
        assert_ne!(unspecified as i32, dont_vet as i32);
        assert_ne!(vet as i32, dont_vet as i32);
    }

    #[test]
    fn force_flag_variants_are_injective() {
        let incompatible: i32 = DamlUpdateVettedPackagesForceFlag::AllowVetIncompatibleUpgrades.into();
        let unvetted: i32 = DamlUpdateVettedPackagesForceFlag::AllowUnvettedDependencies.into();
        assert_ne!(incompatible, unvetted);
    }

    /// `Prior(7)` vs `NoPrior` must remain distinguishable on the
    /// wire — confusing them with each other would either reject
    /// every update (NoPrior on a participant with prior history)
    /// or accept any update (Prior matched against the wrong
    /// serial).
    #[test]
    fn prior_topology_serial_variants_distinct() {
        let prior: PriorTopologySerial = DamlPriorTopologySerial::Prior(7).into();
        let no_prior: PriorTopologySerial = DamlPriorTopologySerial::NoPrior.into();
        match prior.serial {
            Some(Serial::Prior(v)) => assert_eq!(v, 7),
            other => panic!("Prior(7) lost on the way to proto: {other:?}"),
        }
        match no_prior.serial {
            Some(Serial::NoPrior(_)) => {}
            other => panic!("NoPrior lost on the way to proto: {other:?}"),
        }
    }

    #[test]
    fn vetted_packages_ref_roundtrips_to_proto() {
        let dto = DamlVettedPackagesRef {
            package_id: "abc123".to_owned(),
            package_name: "my-pkg".to_owned(),
            package_version: "1.0.0".to_owned(),
        };
        let proto: VettedPackagesRef = dto.clone().into();
        assert_eq!(proto.package_id, dto.package_id);
        assert_eq!(proto.package_name, dto.package_name);
        assert_eq!(proto.package_version, dto.package_version);
    }

    #[test]
    fn vetted_packages_change_vet_serialises() {
        use chrono::TimeZone;

        let from = chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let until = chrono::Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap();
        let dto = DamlVettedPackagesChange::Vet {
            packages: vec![DamlVettedPackagesRef {
                package_id: "abc".to_owned(),
                package_name: "p".to_owned(),
                package_version: "0.1.0".to_owned(),
            }],
            new_valid_from_inclusive: Some(from),
            new_valid_until_exclusive: Some(until),
        };
        let proto: VettedPackagesChange = dto.try_into().expect("convert");
        match proto.operation {
            Some(Operation::Vet(v)) => {
                assert_eq!(v.packages.len(), 1);
                assert_eq!(v.packages[0].package_id, "abc");
                assert!(v.new_valid_from_inclusive.is_some(), "lower bound must be set");
                assert!(v.new_valid_until_exclusive.is_some(), "upper bound must be set");
            }
            other => panic!("Vet variant lost: {other:?}"),
        }
    }

    #[test]
    fn vetted_packages_change_unvet_serialises() {
        let dto = DamlVettedPackagesChange::Unvet {
            packages: vec![DamlVettedPackagesRef::default()],
        };
        let proto: VettedPackagesChange = dto.try_into().expect("convert");
        assert!(matches!(proto.operation, Some(Operation::Unvet(_))));
    }
}
