use std::collections::HashMap;

use crate::data::identifier::DamlIdentifier;
use crate::grpc_protobuf::com::daml::ledger::api::v2::cumulative_filter::IdentifierFilter;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{
    CumulativeFilter, EventFormat, Filters, InterfaceFilter, ParticipantAuthorizationTopologyFormat, TemplateFilter,
    TopologyFormat, TransactionFormat, TransactionShape, UpdateFormat, WildcardFilter,
};

// ---------------------------------------------------------------------------
// Filter atoms: wildcard, interface, template
// ---------------------------------------------------------------------------

/// Match every template. The participant ships every contract event
/// visible to the requesting parties; pair with
/// [`DamlEventFormat::filters_by_party`] (or `filters_for_any_party`)
/// to scope.
///
/// `include_created_event_blob` controls whether matching
/// `CreatedEvent`s carry the opaque blob suitable for forwarding as a
/// `DisclosedContract` in future submissions.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlWildcardFilter {
    pub include_created_event_blob: bool,
}

impl From<DamlWildcardFilter> for WildcardFilter {
    fn from(f: DamlWildcardFilter) -> Self {
        Self {
            include_created_event_blob: f.include_created_event_blob,
        }
    }
}

/// Match contracts that implement a specific interface.
///
/// `include_interface_view = true` makes the participant evaluate the
/// interface's `view` method and attach the result to each matching
/// `CreatedEvent` as a [`DamlInterfaceView`](crate::data::event::DamlInterfaceView).
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlInterfaceFilter {
    pub interface_id: DamlIdentifier,
    pub include_interface_view: bool,
    pub include_created_event_blob: bool,
}

impl From<DamlInterfaceFilter> for InterfaceFilter {
    fn from(f: DamlInterfaceFilter) -> Self {
        Self {
            interface_id: Some(f.interface_id.into()),
            include_interface_view: f.include_interface_view,
            include_created_event_blob: f.include_created_event_blob,
        }
    }
}

/// Match contracts of a specific template.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlTemplateFilter {
    pub template_id: DamlIdentifier,
    pub include_created_event_blob: bool,
}

impl From<DamlTemplateFilter> for TemplateFilter {
    fn from(f: DamlTemplateFilter) -> Self {
        Self {
            template_id: Some(f.template_id.into()),
            include_created_event_blob: f.include_created_event_blob,
        }
    }
}

/// One atom in a [`DamlFilters`] cumulative list.
///
/// The proto wraps this as a `oneof identifier_filter` — exactly one
/// of the three variants is set. Multiple `DamlCumulativeFilter`s in a
/// `DamlFilters` are OR-ed: a contract event matches if *any* atom
/// matches.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DamlCumulativeFilter {
    Wildcard(DamlWildcardFilter),
    Interface(DamlInterfaceFilter),
    Template(DamlTemplateFilter),
}

impl From<DamlCumulativeFilter> for CumulativeFilter {
    fn from(f: DamlCumulativeFilter) -> Self {
        Self {
            identifier_filter: Some(match f {
                DamlCumulativeFilter::Wildcard(w) => IdentifierFilter::WildcardFilter(w.into()),
                DamlCumulativeFilter::Interface(i) => IdentifierFilter::InterfaceFilter(i.into()),
                DamlCumulativeFilter::Template(t) => IdentifierFilter::TemplateFilter(t.into()),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Filters: the union/cumulative envelope
// ---------------------------------------------------------------------------

/// A union of [`DamlCumulativeFilter`]s. An event matches the
/// `DamlFilters` if *any* of the cumulative atoms matches; per-atom
/// `include_*` flags are OR-ed on hits.
///
/// An empty filter list defaults to a single wildcard with
/// `include_created_event_blob = false`.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlFilters {
    pub cumulative: Vec<DamlCumulativeFilter>,
}

impl DamlFilters {
    /// Convenience: a single wildcard filter without created-event blobs.
    pub fn wildcard() -> Self {
        Self {
            cumulative: vec![DamlCumulativeFilter::Wildcard(DamlWildcardFilter::default())],
        }
    }
}

impl From<DamlFilters> for Filters {
    fn from(f: DamlFilters) -> Self {
        Self {
            cumulative: f.cumulative.into_iter().map(Into::into).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// EventFormat + TransactionShape + TransactionFormat
// ---------------------------------------------------------------------------

/// Selects whether transaction events are emitted in ACS-delta shape
/// (`Created` + `Archived`) or ledger-effects shape (`Created` +
/// `Exercised`, with full subtree information).
///
/// The proto also has an `Unspecified = 0` value which it explicitly
/// documents as "not intended for actual use" — omitted from this
/// enum.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DamlTransactionShape {
    AcsDelta,
    LedgerEffects,
}

impl From<DamlTransactionShape> for TransactionShape {
    fn from(s: DamlTransactionShape) -> Self {
        match s {
            DamlTransactionShape::AcsDelta => TransactionShape::AcsDelta,
            DamlTransactionShape::LedgerEffects => TransactionShape::LedgerEffects,
        }
    }
}

/// What events to include in an update / ACS / completion stream and
/// what auxiliary data to compute for them.
///
/// `filters_by_party` is keyed by party-id; the value is the filter
/// that applies when that party witnesses an event.
/// `filters_for_any_party` is OR-ed with the per-party filters and
/// applies regardless of party (use it for "I want every event of this
/// shape" queries).
///
/// `verbose = true` makes the participant include human-readable
/// record-field labels in returned values (useful for debugging, more
/// bytes on the wire).
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlEventFormat {
    pub filters_by_party: HashMap<String, DamlFilters>,
    pub filters_for_any_party: Option<DamlFilters>,
    pub verbose: bool,
}

impl From<DamlEventFormat> for EventFormat {
    fn from(f: DamlEventFormat) -> Self {
        Self {
            filters_by_party: f.filters_by_party.into_iter().map(|(k, v)| (k, v.into())).collect(),
            filters_for_any_party: f.filters_for_any_party.map(Into::into),
            verbose: f.verbose,
        }
    }
}

/// Pairs a [`DamlEventFormat`] with a [`DamlTransactionShape`] —
/// required wherever the participant streams transactions (e.g.
/// `UpdateService.GetUpdates`,
/// `CommandService.SubmitAndWaitForTransaction`).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DamlTransactionFormat {
    pub event_format: DamlEventFormat,
    pub transaction_shape: DamlTransactionShape,
}

impl From<DamlTransactionFormat> for TransactionFormat {
    fn from(f: DamlTransactionFormat) -> Self {
        Self {
            event_format: Some(f.event_format.into()),
            transaction_shape: TransactionShape::from(f.transaction_shape) as i32,
        }
    }
}

// ---------------------------------------------------------------------------
// Topology format
// ---------------------------------------------------------------------------

/// Filter for participant-authorization topology events.
/// An empty `parties` list means "every party".
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlParticipantAuthorizationTopologyFormat {
    pub parties: Vec<String>,
}

impl From<DamlParticipantAuthorizationTopologyFormat> for ParticipantAuthorizationTopologyFormat {
    fn from(f: DamlParticipantAuthorizationTopologyFormat) -> Self {
        Self {
            parties: f.parties,
        }
    }
}

/// Filter for the topology-events portion of an [`DamlUpdateFormat`].
/// Leave `include_participant_authorization_events = None` to omit
/// topology events from the stream entirely.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlTopologyFormat {
    pub include_participant_authorization_events: Option<DamlParticipantAuthorizationTopologyFormat>,
}

impl From<DamlTopologyFormat> for TopologyFormat {
    fn from(f: DamlTopologyFormat) -> Self {
        Self {
            include_participant_authorization_events: f.include_participant_authorization_events.map(Into::into),
        }
    }
}

// ---------------------------------------------------------------------------
// UpdateFormat: top-level multiplexer for the update stream
// ---------------------------------------------------------------------------

/// What kinds of updates a subscription should receive. Each kind is
/// independently opt-in: leave any field as `None` to omit that kind
/// of update entirely.
///
/// - `include_transactions` — Daml transactions (with their event
///   shape selected by the inner `DamlTransactionFormat`).
/// - `include_reassignments` — cross-synchronizer (un)assignments.
///   Always emitted in ACS-delta shape regardless of the
///   `DamlEventFormat`'s shape settings.
/// - `include_topology_events` — participant-authorization topology
///   transactions.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlUpdateFormat {
    pub include_transactions: Option<DamlTransactionFormat>,
    pub include_reassignments: Option<DamlEventFormat>,
    pub include_topology_events: Option<DamlTopologyFormat>,
}

impl From<DamlUpdateFormat> for UpdateFormat {
    fn from(f: DamlUpdateFormat) -> Self {
        Self {
            include_transactions: f.include_transactions.map(Into::into),
            include_reassignments: f.include_reassignments.map(Into::into),
            include_topology_events: f.include_topology_events.map(Into::into),
        }
    }
}
