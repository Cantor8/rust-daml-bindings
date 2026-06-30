use std::fmt::Debug;

use chrono::{DateTime, Utc};
use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::DamlResult;
use crate::grpc_protobuf::com::daml::ledger::api::v2::testing::time_service_client::TimeServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::testing::{GetTimeRequest, SetTimeRequest};
use crate::service::common::make_request;
use crate::util;
use crate::util::Required;

/// Read and advance the participant's static-time clock. Only available
/// when the participant is configured with static time
/// (`VersionService::FeaturesDescriptor::experimental::static_time`).
///
/// v2 changes from v1:
/// - `GetTime` is now a unary RPC returning a single timestamp, not a
///   server-streamed sequence. Callers that previously polled the
///   stream should now call `get_time()` on a schedule of their choice.
/// - `GetTimeRequest` and `SetTimeRequest` no longer carry `ledger_id`.
#[derive(Debug)]
pub struct DamlTimeService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

impl<'a> DamlTimeService<'a> {
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

    /// Return the participant's current static time.
    #[instrument(skip(self))]
    pub async fn get_time(&self) -> DamlResult<DateTime<Utc>> {
        let payload = GetTimeRequest {};
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().get_time(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        util::from_grpc_timestamp(&response.current_time.req()?)
    }

    /// Atomically advance the participant's static-time clock from
    /// `current_time` to `new_time`. `current_time` MUST match the
    /// participant's view of the clock at the time of the call (the
    /// participant rejects the request if it doesn't), so this is a
    /// compare-and-set, not a blind set.
    #[instrument(skip(self))]
    pub async fn set_time(
        &self,
        current_time: impl Into<DateTime<Utc>> + Debug,
        new_time: impl Into<DateTime<Utc>> + Debug,
    ) -> DamlResult<()> {
        let payload = SetTimeRequest {
            current_time: Some(util::to_grpc_timestamp(current_time.into())?),
            new_time: Some(util::to_grpc_timestamp(new_time.into())?),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        self.client().set_time(make_request(payload, self.auth_token)?).await?;
        Ok(())
    }

    fn client(&self) -> TimeServiceClient<Channel> {
        TimeServiceClient::new(self.channel.clone())
    }
}
