use std::convert::TryFrom;

use crate::data::DamlTransaction;
use crate::data::completion::DamlOffsetCheckpoint;
use crate::data::reassignment::DamlReassignment;
use crate::data::topology::DamlTopologyTransaction;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::get_update_response::Update as PointUpdateKind;
use crate::grpc_protobuf::com::daml::ledger::api::v2::get_updates_response::Update as StreamUpdateKind;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{GetUpdateResponse, GetUpdatesResponse};
use crate::util::Required;

/// An on-ledger update — one of the three kinds the participant
/// surfaces on its update stream.
///
/// Used as-is by the point-lookup RPCs (`GetUpdateByOffset`,
/// `GetUpdateById`). The streaming `GetUpdates` response also carries
/// `OffsetCheckpoint` markers, so streams yield
/// [`DamlUpdateResponse`] (which wraps either a `DamlUpdate` or a
/// checkpoint).
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlUpdate {
    Transaction(Box<DamlTransaction>),
    Reassignment(Box<DamlReassignment>),
    TopologyTransaction(Box<DamlTopologyTransaction>),
}

impl TryFrom<GetUpdateResponse> for DamlUpdate {
    type Error = DamlError;

    fn try_from(r: GetUpdateResponse) -> DamlResult<Self> {
        Ok(match r.update.req()? {
            PointUpdateKind::Transaction(t) => Self::Transaction(Box::new(DamlTransaction::try_from(t)?)),
            PointUpdateKind::Reassignment(r) => Self::Reassignment(Box::new(DamlReassignment::try_from(r)?)),
            PointUpdateKind::TopologyTransaction(t) => {
                Self::TopologyTransaction(Box::new(DamlTopologyTransaction::try_from(t)?))
            },
        })
    }
}

/// One element in the `GetUpdates` stream: either an actual
/// [`DamlUpdate`] or a periodic [`DamlOffsetCheckpoint`] marker.
///
/// Checkpoints aren't load-bearing for correctness — they let
/// consumers detect command timeouts and to checkpoint their
/// position for resuming the stream later. Most clients will skip
/// past them and match on `Update(_)` only.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlUpdateResponse {
    Update(DamlUpdate),
    OffsetCheckpoint(DamlOffsetCheckpoint),
}

impl TryFrom<GetUpdatesResponse> for DamlUpdateResponse {
    type Error = DamlError;

    fn try_from(r: GetUpdatesResponse) -> DamlResult<Self> {
        Ok(match r.update.req()? {
            StreamUpdateKind::Transaction(t) => {
                Self::Update(DamlUpdate::Transaction(Box::new(DamlTransaction::try_from(t)?)))
            },
            StreamUpdateKind::Reassignment(r) => {
                Self::Update(DamlUpdate::Reassignment(Box::new(DamlReassignment::try_from(r)?)))
            },
            StreamUpdateKind::TopologyTransaction(t) => {
                Self::Update(DamlUpdate::TopologyTransaction(Box::new(DamlTopologyTransaction::try_from(t)?)))
            },
            StreamUpdateKind::OffsetCheckpoint(c) => Self::OffsetCheckpoint(DamlOffsetCheckpoint::try_from(c)?),
        })
    }
}
