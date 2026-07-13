use std::convert::TryFrom;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::data::offset::DamlLedgerOffset;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::completion::DeduplicationPeriod;
use crate::grpc_protobuf::com::daml::ledger::api::v2::completion_stream_response::CompletionResponse;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{
    Completion, CompletionStreamResponse, OffsetCheckpoint, SynchronizerTime,
};
use crate::grpc_protobuf::google::rpc::Status;
use crate::util;
use crate::util::Required;

/// One element in a `CompletionStream`. v2 sends *either* a [`DamlCompletion`]
/// (the participant's verdict on a submission) *or* a
/// [`DamlOffsetCheckpoint`] (a periodic offset marker used to detect
/// timeouts and to checkpoint stream resumption); never both in the same
/// message.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlCompletionResponse {
    Completion(DamlCompletion),
    OffsetCheckpoint(DamlOffsetCheckpoint),
}

impl TryFrom<CompletionStreamResponse> for DamlCompletionResponse {
    type Error = DamlError;

    fn try_from(response: CompletionStreamResponse) -> DamlResult<Self> {
        match response.completion_response.req()? {
            CompletionResponse::Completion(c) => Ok(Self::Completion(DamlCompletion::try_from(c)?)),
            CompletionResponse::OffsetCheckpoint(c) => Ok(Self::OffsetCheckpoint(DamlOffsetCheckpoint::try_from(c)?)),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlCompletion {
    pub command_id: String,
    pub status: DamlStatus,
    /// The id of the resulting transaction or reassignment. v2 generalised
    /// v1's `transaction_id` because a command may now produce a
    /// non-transaction update (e.g. a reassignment).
    pub update_id: String,
    /// v2 renames v1's `application_id`.
    pub user_id: String,
    pub act_as: Vec<String>,
    pub submission_id: String,
    pub deduplication_period: Option<DamlCompletionDeduplicationPeriod>,
    /// Offset at which the participant emitted this completion. Use this
    /// in a follow-up `CompletionStreamRequest::begin_exclusive` to resume
    /// the stream after a disconnect.
    pub offset: DamlLedgerOffset,
    /// The synchronizer that ordered the underlying confirmation request,
    /// plus its record time at the corresponding offset.
    pub synchronizer_time: Option<DamlSynchronizerTime>,
    /// Traffic cost paid by this participant for the submission. Zero for
    /// pre-ordering rejections; see proto docs for caveats.
    pub paid_traffic_cost: i64,
}

impl TryFrom<Completion> for DamlCompletion {
    type Error = DamlError;

    fn try_from(c: Completion) -> DamlResult<Self> {
        Ok(Self {
            command_id: c.command_id,
            // Per proto, `status` is documented as optional but is set on
            // every completion the participant emits in practice — treat
            // absence as a wire-protocol violation.
            status: DamlStatus::from(c.status.req()?),
            update_id: c.update_id,
            user_id: c.user_id,
            act_as: c.act_as,
            submission_id: c.submission_id,
            deduplication_period: c
                .deduplication_period
                .map(DamlCompletionDeduplicationPeriod::try_from)
                .transpose()?,
            offset: DamlLedgerOffset::new(c.offset),
            synchronizer_time: c.synchronizer_time.map(DamlSynchronizerTime::try_from).transpose()?,
            paid_traffic_cost: c.paid_traffic_cost,
        })
    }
}

/// Periodic offset marker emitted in the completion (and update) streams.
/// Lets clients (a) detect commands that have likely timed out (no
/// completion received before `synchronizer_times` advanced past the
/// command's max record time) and (b) checkpoint stream position so a
/// later subscription can resume from the same point.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlOffsetCheckpoint {
    pub offset: DamlLedgerOffset,
    pub synchronizer_times: Vec<DamlSynchronizerTime>,
}

impl TryFrom<OffsetCheckpoint> for DamlOffsetCheckpoint {
    type Error = DamlError;

    fn try_from(c: OffsetCheckpoint) -> DamlResult<Self> {
        Ok(Self {
            offset: DamlLedgerOffset::new(c.offset),
            synchronizer_times: c
                .synchronizer_times
                .into_iter()
                .map(DamlSynchronizerTime::try_from)
                .collect::<DamlResult<Vec<_>>>()?,
        })
    }
}

/// A `(synchronizer_id, record_time)` pair, attached to checkpoints and
/// completions so clients can reason about per-synchronizer freshness.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlSynchronizerTime {
    pub synchronizer_id: String,
    pub record_time: DateTime<Utc>,
}

impl TryFrom<SynchronizerTime> for DamlSynchronizerTime {
    type Error = DamlError;

    fn try_from(s: SynchronizerTime) -> DamlResult<Self> {
        Ok(Self {
            synchronizer_id: s.synchronizer_id,
            record_time: util::from_grpc_timestamp(&s.record_time.req()?)?,
        })
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlStatus {
    pub code: i32,
    pub message: String,
    /// Structured error details as a list of `google.protobuf.Any`.
    /// Downstream decoders can match on `type_url` to recover the
    /// concrete Daml error payload (e.g. `com.daml.error.ErrorInfo`).
    pub details: Vec<prost_types::Any>,
}

impl From<Status> for DamlStatus {
    fn from(status: Status) -> Self {
        Self {
            code: status.code,
            message: status.message,
            details: status.details,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlCompletionDeduplicationPeriod {
    /// Completion-stream offset (exclusive). v2 changed this from a
    /// stringified offset to a real `int64`.
    DeduplicationOffset(i64),
    DeduplicationDuration(Duration),
}

impl TryFrom<DeduplicationPeriod> for DamlCompletionDeduplicationPeriod {
    type Error = DamlError;

    fn try_from(p: DeduplicationPeriod) -> DamlResult<Self> {
        Ok(match p {
            DeduplicationPeriod::DeduplicationOffset(offset) => Self::DeduplicationOffset(offset),
            DeduplicationPeriod::DeduplicationDuration(duration) => {
                Self::DeduplicationDuration(util::from_grpc_duration(&duration)?)
            },
        })
    }
}
