use std::convert::TryFrom;
use std::fmt::Debug;

use futures::{Stream, StreamExt};
use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::filter::DamlUpdateFormat;
use crate::data::offset::DamlLedgerOffset;
use crate::data::update::{DamlUpdate, DamlUpdateResponse};
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::update_service_client::UpdateServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::{
    GetUpdateByIdRequest, GetUpdateByOffsetRequest, GetUpdatesPageRequest, GetUpdatesRequest,
};
use crate::service::common::make_request;

/// Read the participant's update stream — Daml transactions,
/// cross-synchronizer reassignments, and topology transactions —
/// indexed by participant offset.
///
/// v2 unified what v1 split across `TransactionService`,
/// `ActiveContractsService`-flavoured reads, and topology
/// notifications. Within a single synchronizer the offset ordering is
/// strictly causal; across synchronizers it isn't.
#[derive(Debug)]
pub struct DamlUpdateService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

/// One page of `GetUpdatesPage` results. `next_page_token` is empty
/// on the last page. Subsequent paging requests must echo the same
/// `begin/end/update_format/descending_order` from the original call.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlUpdatesPage {
    pub updates: Vec<DamlUpdate>,
    pub lowest_page_offset_exclusive: DamlLedgerOffset,
    pub highest_page_offset_inclusive: DamlLedgerOffset,
    /// Page tokens are opaque server-issued bytes; pass back verbatim.
    pub next_page_token: Option<Vec<u8>>,
}

impl<'a> DamlUpdateService<'a> {
    pub fn new(channel: Channel, auth_token: Option<&'a str>) -> Self {
        Self {
            channel,
            auth_token,
        }
    }

    /// Override the JWT token to use for this service.
    pub fn with_token(self, auth_token: &'a str) -> Self {
        Self {
            auth_token: Some(auth_token),
            ..self
        }
    }

    /// Subscribe to the update stream.
    ///
    /// * `begin_exclusive` — only updates with offsets strictly
    ///   greater than this are returned. `BEGIN` (`0`) starts at the
    ///   ledger's first offset. If the participant has been pruned,
    ///   this must be at least the pruning offset.
    /// * `end_inclusive` — `None` produces an open-ended stream that
    ///   keeps yielding as new updates land. `Some(_)` produces a
    ///   bounded stream that terminates after the named offset.
    /// * `update_format` — what kinds of updates to include, what
    ///   shape (ACS-delta vs ledger-effects), per-party filters, etc.
    /// * `descending_order` — only meaningful when `end_inclusive` is
    ///   `Some(_)` (open streams must be ascending).
    ///
    /// The stream yields one [`DamlUpdateResponse`] per message —
    /// either an `Update(_)` or an `OffsetCheckpoint(_)`.
    #[instrument(skip(self))]
    pub async fn get_updates(
        &self,
        begin_exclusive: impl Into<DamlLedgerOffset> + Debug,
        end_inclusive: Option<DamlLedgerOffset>,
        update_format: DamlUpdateFormat,
        descending_order: bool,
    ) -> DamlResult<impl Stream<Item = DamlResult<DamlUpdateResponse>>> {
        let payload = GetUpdatesRequest {
            begin_exclusive: begin_exclusive.into().value(),
            end_inclusive: end_inclusive.map(DamlLedgerOffset::value),
            update_format: Some(update_format.into()),
            descending_order,
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let stream = self.client().get_updates(make_request(payload, self.auth_token)?).await?.into_inner();
        Ok(stream.inspect(|r| trace!(?r)).map(|item| match item {
            Ok(response) => DamlUpdateResponse::try_from(response),
            Err(e) => Err(DamlError::from(e)),
        }))
    }

    /// Look up a specific update by participant offset.
    ///
    /// Errors with `UPDATE_NOT_FOUND` when no update exists at that
    /// offset, or when every event would be filtered out by
    /// `update_format`.
    #[instrument(skip(self))]
    pub async fn get_update_by_offset(
        &self,
        offset: impl Into<DamlLedgerOffset> + Debug,
        update_format: DamlUpdateFormat,
    ) -> DamlResult<DamlUpdate> {
        let payload = GetUpdateByOffsetRequest {
            offset: offset.into().value(),
            update_format: Some(update_format.into()),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().get_update_by_offset(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        DamlUpdate::try_from(response)
    }

    /// Look up a specific update by its `update_id`. Same error
    /// behaviour as `get_update_by_offset` if not found or fully
    /// filtered.
    #[instrument(skip(self))]
    pub async fn get_update_by_id(
        &self,
        update_id: impl Into<String> + Debug,
        update_format: DamlUpdateFormat,
    ) -> DamlResult<DamlUpdate> {
        let payload = GetUpdateByIdRequest {
            update_id: update_id.into(),
            update_format: Some(update_format.into()),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().get_update_by_id(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        DamlUpdate::try_from(response)
    }

    /// Fetch one page of updates.
    ///
    /// Useful when the historical slice is large enough that you want
    /// bounded memory usage. `page_token = None` returns the first
    /// page; loop until the returned `next_page_token` is `None`.
    /// Subsequent requests in the same paging session must echo the
    /// same `begin_offset_exclusive`, `end_offset_inclusive`,
    /// `update_format`, and `descending_order`.
    #[instrument(skip(self))]
    pub async fn get_updates_page(
        &self,
        begin_offset_exclusive: Option<DamlLedgerOffset>,
        end_offset_inclusive: Option<DamlLedgerOffset>,
        max_page_size: Option<i32>,
        update_format: DamlUpdateFormat,
        descending_order: bool,
        page_token: Option<Vec<u8>>,
    ) -> DamlResult<DamlUpdatesPage> {
        let payload = GetUpdatesPageRequest {
            begin_offset_exclusive: begin_offset_exclusive.map(DamlLedgerOffset::value),
            end_offset_inclusive: end_offset_inclusive.map(DamlLedgerOffset::value),
            max_page_size,
            update_format: Some(update_format.into()),
            descending_order,
            page_token,
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().get_updates_page(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlUpdatesPage {
            updates: response.updates.into_iter().map(DamlUpdate::try_from).collect::<DamlResult<_>>()?,
            lowest_page_offset_exclusive: DamlLedgerOffset::new(response.lowest_page_offset_exclusive),
            highest_page_offset_inclusive: DamlLedgerOffset::new(response.highest_page_offset_inclusive),
            next_page_token: response.next_page_token,
        })
    }

    fn client(&self) -> UpdateServiceClient<Channel> {
        UpdateServiceClient::new(self.channel.clone())
    }
}
