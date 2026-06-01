use std::convert::TryFrom;

use chrono::{DateTime, Utc};

use crate::data::event::DamlEvent;
use crate::data::offset::DamlLedgerOffset;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::Transaction;
use crate::util;
use crate::util::Required;

/// A filtered view of a Daml ledger transaction.
///
/// v2 changes from v1:
///   - `transaction_id` is renamed `update_id` (a transaction is one
///     kind of update; reassignments and topology transactions are
///     the others).
///   - `offset` is now an `int64` (modelled here as the
///     [`DamlLedgerOffset`] newtype).
///   - `synchronizer_id`, `record_time`, `external_transaction_hash`
///     (for externally-signed submissions), and `paid_traffic_cost`
///     are new.
///   - `command_id` is now `Option`-shaped: the participant only
///     populates it for the submitting party. Modelled here as
///     `String` with empty string meaning "absent", matching the
///     proto's wire treatment.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DamlTransaction {
    pub update_id: String,
    /// Empty when the receiving party didn't submit this transaction.
    pub command_id: String,
    pub workflow_id: String,
    /// Ledger effective time of the transaction.
    pub effective_at: DateTime<Utc>,
    pub events: Vec<DamlEvent>,
    pub offset: DamlLedgerOffset,
    pub synchronizer_id: String,
    /// Record time at which the synchronizer ordered this transaction.
    pub record_time: DateTime<Utc>,
    /// Hash signed by the external party — present for externally
    /// signed submissions. Use it to correlate the original signed
    /// payload with the resulting on-ledger transaction.
    pub external_transaction_hash: Option<Vec<u8>>,
    /// Traffic cost paid by this participant for ordering the
    /// transaction's confirmation request. Absent for transactions
    /// the participant didn't initiate, repair-service transactions,
    /// and queries scoped to a non-submitting party.
    pub paid_traffic_cost: Option<i64>,
}

impl TryFrom<Transaction> for DamlTransaction {
    type Error = DamlError;

    fn try_from(tx: Transaction) -> DamlResult<Self> {
        Ok(Self {
            update_id: tx.update_id,
            command_id: tx.command_id,
            workflow_id: tx.workflow_id,
            effective_at: util::from_grpc_timestamp(&tx.effective_at.req()?),
            events: tx.events.into_iter().map(DamlEvent::try_from).collect::<DamlResult<_>>()?,
            offset: DamlLedgerOffset::new(tx.offset),
            synchronizer_id: tx.synchronizer_id,
            record_time: util::from_grpc_timestamp(&tx.record_time.req()?),
            external_transaction_hash: tx.external_transaction_hash,
            paid_traffic_cost: tx.paid_traffic_cost,
        })
    }
}
