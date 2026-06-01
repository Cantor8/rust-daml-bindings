use std::convert::TryFrom;

use crate::data::event::{DamlArchivedEvent, DamlCreatedEvent};
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::{Archived, Created, GetEventsByContractIdResponse};
use crate::util::Required;

/// A create event scoped to a single synchronizer (the one that
/// sequenced it). EventQueryService surfaces this richer shape because
/// the same contract can appear on multiple synchronizers — clients
/// reading event history need the synchronizer id to disambiguate.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlCreated {
    pub created_event: DamlCreatedEvent,
    pub synchronizer_id: String,
}

impl TryFrom<Created> for DamlCreated {
    type Error = DamlError;

    fn try_from(c: Created) -> DamlResult<Self> {
        Ok(Self {
            created_event: DamlCreatedEvent::try_from(c.created_event.req()?)?,
            synchronizer_id: c.synchronizer_id,
        })
    }
}

/// An archive event scoped to its sequencing synchronizer.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlArchived {
    pub archived_event: DamlArchivedEvent,
    pub synchronizer_id: String,
}

impl TryFrom<Archived> for DamlArchived {
    type Error = DamlError;

    fn try_from(a: Archived) -> DamlResult<Self> {
        Ok(Self {
            archived_event: DamlArchivedEvent::try_from(a.archived_event.req()?)?,
            synchronizer_id: a.synchronizer_id,
        })
    }
}

/// Result of [`get_events_by_contract_id`]. Both halves are `Option`:
/// `created` is missing when the create has been pruned, `archived`
/// is missing when the contract hasn't been archived yet (or that
/// archive has been pruned).
///
/// Note: an entirely empty response (`created = None, archived =
/// None`) is not actually returned — the participant surfaces
/// `CONTRACT_EVENTS_NOT_FOUND` instead. The Optional fields exist for
/// the partial-pruning cases.
///
/// [`get_events_by_contract_id`]: crate::service::DamlEventQueryService::get_events_by_contract_id
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlEventsByContractId {
    pub created: Option<DamlCreated>,
    pub archived: Option<DamlArchived>,
}

impl TryFrom<GetEventsByContractIdResponse> for DamlEventsByContractId {
    type Error = DamlError;

    fn try_from(r: GetEventsByContractIdResponse) -> DamlResult<Self> {
        Ok(Self {
            created: r.created.map(DamlCreated::try_from).transpose()?,
            archived: r.archived.map(DamlArchived::try_from).transpose()?,
        })
    }
}
