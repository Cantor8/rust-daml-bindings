use std::convert::TryFrom;

use crate::data::identifier::DamlIdentifier;
use crate::data::offset::DamlLedgerOffset;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::ArchivedEvent;
use crate::util::Required;

/// Records that a contract was archived. v2 dropped `event_id`,
/// gained `(offset, node_id)` for addressing, `package_name`, and
/// `implemented_interfaces`.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlArchivedEvent {
    pub offset: DamlLedgerOffset,
    pub node_id: i32,
    pub contract_id: String,
    /// Template that defines the choice that archived the contract.
    /// May differ from the target contract's own template-id under
    /// smart-contract upgrades.
    pub template_id: DamlIdentifier,
    pub witness_parties: Vec<String>,
    pub package_name: String,
    /// Interfaces implemented by the target template that matched the
    /// transaction filter's interface filters (when
    /// `include_interface_view` was set).
    pub implemented_interfaces: Vec<DamlIdentifier>,
}

impl TryFrom<ArchivedEvent> for DamlArchivedEvent {
    type Error = DamlError;

    fn try_from(e: ArchivedEvent) -> DamlResult<Self> {
        Ok(Self {
            offset: DamlLedgerOffset::new(e.offset),
            node_id: e.node_id,
            contract_id: e.contract_id,
            template_id: DamlIdentifier::from(e.template_id.req()?),
            witness_parties: e.witness_parties,
            package_name: e.package_name,
            implemented_interfaces: e.implemented_interfaces.into_iter().map(DamlIdentifier::from).collect(),
        })
    }
}
