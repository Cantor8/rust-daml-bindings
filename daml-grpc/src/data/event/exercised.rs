use std::convert::TryFrom;

use crate::data::identifier::DamlIdentifier;
use crate::data::offset::DamlLedgerOffset;
use crate::data::value::DamlValue;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::ExercisedEvent;
use crate::util::Required;

/// Records that a choice was exercised on a contract.
///
/// v2 changes from v1:
///   - `event_id` is gone; events are addressed by `(offset, node_id)`.
///   - `child_event_ids` is replaced by `last_descendant_node_id`,
///     which lets clients identify the whole subtree without
///     enumerating it explicitly.
///   - `exercise_result` is now optional (a non-consuming exercise
///     can return Unit, which the wire treats as missing).
///   - `interface_id`, `implemented_interfaces`, `package_name`, and
///     `acs_delta` are new.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlExercisedEvent {
    pub offset: DamlLedgerOffset,
    pub node_id: i32,
    pub contract_id: String,
    pub template_id: DamlIdentifier,
    /// Set when the choice was exercised via an interface; identifies
    /// the interface in which the choice is defined. `None` when the
    /// choice was exercised directly on the template.
    pub interface_id: Option<DamlIdentifier>,
    pub choice: String,
    pub choice_argument: DamlValue,
    pub acting_parties: Vec<String>,
    pub consuming: bool,
    pub witness_parties: Vec<String>,
    /// Upper bound (inclusive) on node ids of events in the same
    /// transaction that descend from this exercise — i.e. the rooted
    /// subtree spans `[node_id, last_descendant_node_id]`.
    pub last_descendant_node_id: i32,
    /// Result of the exercise. `None` when the choice returned Unit
    /// (the wire omits the field in that case).
    pub exercise_result: Option<DamlValue>,
    pub package_name: String,
    /// Interfaces implemented by the target template that matched the
    /// transaction filter's interface filters. Only populated when the
    /// exercise was consuming and `include_interface_view` was set.
    pub implemented_interfaces: Vec<DamlIdentifier>,
    pub acs_delta: bool,
}

impl TryFrom<ExercisedEvent> for DamlExercisedEvent {
    type Error = DamlError;

    fn try_from(e: ExercisedEvent) -> DamlResult<Self> {
        Ok(Self {
            offset: DamlLedgerOffset::new(e.offset),
            node_id: e.node_id,
            contract_id: e.contract_id,
            template_id: DamlIdentifier::from(e.template_id.req()?),
            interface_id: e.interface_id.map(DamlIdentifier::from),
            choice: e.choice,
            choice_argument: DamlValue::try_from(e.choice_argument.req()?)?,
            acting_parties: e.acting_parties,
            consuming: e.consuming,
            witness_parties: e.witness_parties,
            last_descendant_node_id: e.last_descendant_node_id,
            exercise_result: e.exercise_result.map(DamlValue::try_from).transpose()?,
            package_name: e.package_name,
            implemented_interfaces: e.implemented_interfaces.into_iter().map(DamlIdentifier::from).collect(),
            acs_delta: e.acs_delta,
        })
    }
}
