use std::convert::TryFrom;

use chrono::{DateTime, Utc};

use crate::data::completion::DamlStatus;
use crate::data::identifier::DamlIdentifier;
use crate::data::offset::DamlLedgerOffset;
use crate::data::value::{DamlRecord, DamlValue};
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::{CreatedEvent, InterfaceView};
use crate::util;
use crate::util::Required;

/// Records that a contract was created.
///
/// v2 reshaped this event heavily versus v1:
///   - `event_id` and `agreement_text` are gone. Events are addressed
///     by the `(offset, node_id)` pair instead.
///   - `interface_views`, `created_event_blob`, `contract_key_hash`,
///     `created_at`, `package_name`, `acs_delta`, and
///     `representative_package_id` are new.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlCreatedEvent {
    /// Participant-local offset at which this event was emitted.
    pub offset: DamlLedgerOffset,
    /// Position of this event within the originating transaction or
    /// reassignment.
    pub node_id: i32,
    pub contract_id: String,
    pub template_id: DamlIdentifier,
    pub contract_key: Option<DamlValue>,
    /// Hash of the contract key (present iff `template_id` defines a
    /// contract key).
    pub contract_key_hash: Vec<u8>,
    pub create_arguments: DamlRecord,
    /// Opaque payload for forwarding this event as a `DisclosedContract`
    /// in a future command submission.
    pub created_event_blob: Vec<u8>,
    /// Interface views requested via `InterfaceFilter::include_interface_view`.
    pub interface_views: Vec<DamlInterfaceView>,
    pub witness_parties: Vec<String>,
    pub signatories: Vec<String>,
    pub observers: Vec<String>,
    /// Ledger-effective time of the creating transaction.
    pub created_at: DateTime<Utc>,
    /// Package-name of the created contract.
    pub package_name: String,
    /// Whether this event would appear on an ACS-delta-shaped stream.
    /// Tracks contract activeness on the client side.
    pub acs_delta: bool,
    /// Server-internal: a package-id from the participant's store that
    /// typechecks the contract's arguments. May differ from the
    /// template's package-id when upgrades have happened. Documented
    /// as experimental and "not for client consumption" — surfaced
    /// anyway for round-trip parity.
    pub representative_package_id: String,
}

impl TryFrom<CreatedEvent> for DamlCreatedEvent {
    type Error = DamlError;

    fn try_from(e: CreatedEvent) -> DamlResult<Self> {
        Ok(Self {
            offset: DamlLedgerOffset::new(e.offset),
            node_id: e.node_id,
            contract_id: e.contract_id,
            template_id: DamlIdentifier::from(e.template_id.req()?),
            contract_key: e.contract_key.map(DamlValue::try_from).transpose()?,
            contract_key_hash: e.contract_key_hash,
            create_arguments: DamlRecord::try_from(e.create_arguments.req()?)?,
            created_event_blob: e.created_event_blob,
            interface_views: e
                .interface_views
                .into_iter()
                .map(DamlInterfaceView::try_from)
                .collect::<DamlResult<_>>()?,
            witness_parties: e.witness_parties,
            signatories: e.signatories,
            observers: e.observers,
            created_at: util::from_grpc_timestamp(&e.created_at.req()?)?,
            package_name: e.package_name,
            acs_delta: e.acs_delta,
            representative_package_id: e.representative_package_id,
        })
    }
}

/// View of a created event matched by an interface filter — the
/// participant evaluates the interface's `view` method for each
/// matching event and ships the result alongside.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlInterfaceView {
    pub interface_id: DamlIdentifier,
    /// The result of evaluating the view: `code == 0` is success;
    /// otherwise the view computation failed and `view_value` will be
    /// `None`.
    pub view_status: DamlStatus,
    /// Computed view value. `None` when `view_status` reports an error.
    pub view_value: Option<DamlRecord>,
    /// Package that supplied the interface implementation used to
    /// compute the view. Empty when the computation failed.
    pub implementation_package_id: String,
}

impl TryFrom<InterfaceView> for DamlInterfaceView {
    type Error = DamlError;

    fn try_from(v: InterfaceView) -> DamlResult<Self> {
        Ok(Self {
            interface_id: DamlIdentifier::from(v.interface_id.req()?),
            view_status: DamlStatus::from(v.view_status.req()?),
            view_value: v.view_value.map(DamlRecord::try_from).transpose()?,
            implementation_package_id: v.implementation_package_id,
        })
    }
}
