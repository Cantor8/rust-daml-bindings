use std::convert::TryFrom;

use crate::data::event::{DamlArchivedEvent, DamlCreatedEvent, DamlExercisedEvent};
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::event::Event as EventKind;
use crate::grpc_protobuf::com::daml::ledger::api::v2::Event;
use crate::util::Required;

/// A Daml ledger event as carried by the v2 update stream.
///
/// v2 unified v1's two stream shapes into a single envelope:
///   - On an ACS-delta-shaped stream you see `Created` and `Archived`.
///   - On a ledger-effects-shaped stream you see `Created` and
///     `Exercised`.
///
/// The shape is selected by `TransactionFormat::transaction_shape`.
/// v1's separate `TreeEvent` envelope is gone — its purpose is now
/// served by the ledger-effects shape, which is just `DamlEvent` with
/// the `Exercised` variant.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlEvent {
    Created(Box<DamlCreatedEvent>),
    Archived(Box<DamlArchivedEvent>),
    Exercised(Box<DamlExercisedEvent>),
}

impl DamlEvent {
    pub fn try_created(self) -> DamlResult<DamlCreatedEvent> {
        match self {
            DamlEvent::Created(e) => Ok(*e),
            _ => Err(self.make_unexpected_type_error("Created")),
        }
    }

    pub fn try_archived(self) -> DamlResult<DamlArchivedEvent> {
        match self {
            DamlEvent::Archived(e) => Ok(*e),
            _ => Err(self.make_unexpected_type_error("Archived")),
        }
    }

    pub fn try_exercised(self) -> DamlResult<DamlExercisedEvent> {
        match self {
            DamlEvent::Exercised(e) => Ok(*e),
            _ => Err(self.make_unexpected_type_error("Exercised")),
        }
    }

    pub fn variant_name(&self) -> &str {
        match self {
            DamlEvent::Created(_) => "Created",
            DamlEvent::Archived(_) => "Archived",
            DamlEvent::Exercised(_) => "Exercised",
        }
    }

    /// The `(offset, node_id)` pair identifying this event within the
    /// participant's view.
    pub fn address(&self) -> (crate::data::offset::DamlLedgerOffset, i32) {
        match self {
            DamlEvent::Created(c) => (c.offset, c.node_id),
            DamlEvent::Archived(a) => (a.offset, a.node_id),
            DamlEvent::Exercised(e) => (e.offset, e.node_id),
        }
    }

    fn make_unexpected_type_error(&self, expected: &str) -> DamlError {
        DamlError::UnexpectedType(expected.to_owned(), self.variant_name().to_owned())
    }
}

impl TryFrom<Event> for DamlEvent {
    type Error = DamlError;

    fn try_from(event: Event) -> DamlResult<Self> {
        Ok(match event.event.req()? {
            EventKind::Created(e) => DamlEvent::Created(Box::new(DamlCreatedEvent::try_from(e)?)),
            EventKind::Archived(e) => DamlEvent::Archived(Box::new(DamlArchivedEvent::try_from(e)?)),
            EventKind::Exercised(e) => DamlEvent::Exercised(Box::new(DamlExercisedEvent::try_from(e)?)),
        })
    }
}
