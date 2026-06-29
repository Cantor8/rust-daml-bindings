use std::time::Duration;

use tonic::transport::{Certificate, Channel, ClientTlsConfig};
#[cfg(test)]
use tonic::transport::Uri;
use tracing::{debug, instrument};

use crate::data::{DamlError, DamlResult};
use crate::service::{
    DamlCommandCompletionService, DamlCommandService, DamlCommandSubmissionService, DamlContractService,
    DamlEventQueryService, DamlPackageService, DamlParticipantPruningService, DamlStateService, DamlUpdateService,
    DamlVersionService,
};
#[cfg(feature = "admin")]
use crate::service::{
    DamlCommandInspectionService, DamlIdentityProviderConfigService, DamlPackageManagementService,
    DamlPartyManagementService, DamlUserManagementService,
};
use crate::service::DamlTimeService;

const DEFAULT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 5;

/// Connection configuration for a [`DamlGrpcClient`].
///
/// v2 dropped v1's `ledger_id` discovery flow — every request runs
/// against the connected participant directly, so there's no
/// `LedgerIdentityService` round-trip at connect time and no
/// reset-and-wait timeout needed for that flow.
#[derive(Debug, Default)]
pub struct DamlGrpcClientConfig {
    uri: String,
    timeout: Duration,
    connect_timeout: Option<Duration>,
    concurrency_limit: Option<usize>,
    rate_limit: Option<(u64, Duration)>,
    initial_stream_window_size: Option<u32>,
    initial_connection_window_size: Option<u32>,
    tcp_keepalive: Option<Duration>,
    tcp_nodelay: bool,
    tls_config: Option<DamlGrpcTlsConfig>,
    auth_token: Option<String>,
}

#[derive(Debug)]
pub struct DamlGrpcTlsConfig {
    ca_cert: Option<Vec<u8>>,
}

/// Construct a [`DamlGrpcClient`].
pub struct DamlGrpcClientBuilder {
    config: DamlGrpcClientConfig,
}

impl DamlGrpcClientBuilder {
    pub fn uri(uri: impl Into<String>) -> Self {
        Self {
            config: DamlGrpcClientConfig {
                uri: uri.into(),
                timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
                connect_timeout: Some(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS)),
                ..DamlGrpcClientConfig::default()
            },
        }
    }

    /// The network timeout.
    pub fn timeout(self, timeout: Duration) -> Self {
        Self {
            config: DamlGrpcClientConfig {
                timeout,
                ..self.config
            },
        }
    }

    /// The connection timeout.
    pub fn connect_timeout(self, connect_timeout: Option<Duration>) -> Self {
        Self {
            config: DamlGrpcClientConfig {
                connect_timeout,
                ..self.config
            },
        }
    }

    pub fn concurrency_limit(self, concurrency_limit: usize) -> Self {
        Self {
            config: DamlGrpcClientConfig {
                concurrency_limit: Some(concurrency_limit),
                ..self.config
            },
        }
    }

    pub fn rate_limit(self, rate_limit: (u64, Duration)) -> Self {
        Self {
            config: DamlGrpcClientConfig {
                rate_limit: Some(rate_limit),
                ..self.config
            },
        }
    }

    pub fn initial_stream_window_size(self, initial_stream_window_size: u32) -> Self {
        Self {
            config: DamlGrpcClientConfig {
                initial_stream_window_size: Some(initial_stream_window_size),
                ..self.config
            },
        }
    }

    pub fn initial_connection_window_size(self, initial_connection_window_size: u32) -> Self {
        Self {
            config: DamlGrpcClientConfig {
                initial_connection_window_size: Some(initial_connection_window_size),
                ..self.config
            },
        }
    }

    pub fn tcp_keepalive(self, tcp_keepalive: Duration) -> Self {
        Self {
            config: DamlGrpcClientConfig {
                tcp_keepalive: Some(tcp_keepalive),
                ..self.config
            },
        }
    }

    pub fn tcp_nodelay(self, tcp_nodelay: bool) -> Self {
        Self {
            config: DamlGrpcClientConfig {
                tcp_nodelay,
                ..self.config
            },
        }
    }

    pub fn with_tls(self, ca_cert: impl Into<Vec<u8>>) -> Self {
        Self {
            config: DamlGrpcClientConfig {
                tls_config: Some(DamlGrpcTlsConfig {
                    ca_cert: Some(ca_cert.into()),
                }),
                ..self.config
            },
        }
    }

    pub fn with_auth(self, auth_token: String) -> Self {
        Self {
            config: DamlGrpcClientConfig {
                auth_token: Some(auth_token),
                ..self.config
            },
        }
    }

    pub async fn connect(self) -> DamlResult<DamlGrpcClient> {
        DamlGrpcClient::connect(self.config).await
    }
}

/// Daml v2 ledger client connection. A thin handle around a tonic
/// [`Channel`] that hands out per-service clients on demand. Cheap to
/// hold — service factories clone the channel rather than opening
/// new ones.
#[derive(Debug)]
pub struct DamlGrpcClient {
    config: DamlGrpcClientConfig,
    channel: Channel,
}

impl DamlGrpcClient {
    /// Open a channel and connect.
    #[instrument]
    pub async fn connect(config: DamlGrpcClientConfig) -> DamlResult<Self> {
        debug!("connecting to {}", config.uri);
        let channel = Self::make_channel(&config).await?;
        Ok(Self {
            config,
            channel,
        })
    }

    pub const fn config(&self) -> &DamlGrpcClientConfig {
        &self.config
    }

    /// Retrieve a [`DamlPackageService`] for querying the Daml-LF
    /// packages supported by the participant.
    pub fn package_service(&self) -> DamlPackageService<'_> {
        DamlPackageService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlCommandSubmissionService`] for fire-and-forget
    /// command submissions. Completion is observed separately through
    /// [`command_completion_service`](Self::command_completion_service).
    pub fn command_submission_service(&self) -> DamlCommandSubmissionService<'_> {
        DamlCommandSubmissionService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlCommandCompletionService`] for observing the
    /// asynchronous outcome (success or rejection) of command
    /// submissions, plus periodic `OffsetCheckpoint` markers.
    pub fn command_completion_service(&self) -> DamlCommandCompletionService<'_> {
        DamlCommandCompletionService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlUpdateService`] for reading the participant's
    /// update stream — transactions, reassignments, and topology
    /// transactions, paginated or open-ended. v2's replacement for
    /// v1's TransactionService.
    pub fn update_service(&self) -> DamlUpdateService<'_> {
        DamlUpdateService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlStateService`] for snapshotting the active
    /// contract set, listing connected synchronizers, reading the
    /// ledger end, and querying pruning watermarks. v2's
    /// replacement for v1's ActiveContractsService.
    pub fn state_service(&self) -> DamlStateService<'_> {
        DamlStateService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlEventQueryService`] for per-contract event
    /// lookup (create + consuming-archive halves) by contract id.
    pub fn event_query_service(&self) -> DamlEventQueryService<'_> {
        DamlEventQueryService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlContractService`] for contract-payload
    /// lookup by id. Experimental / alpha per the proto; prefer
    /// [`event_query_service`](Self::event_query_service) or
    /// [`state_service`](Self::state_service) for stable surfaces.
    pub fn contract_service(&self) -> DamlContractService<'_> {
        DamlContractService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlCommandService`] for synchronous command
    /// submission: submit and wait for the participant's verdict in
    /// a single RPC.
    pub fn command_service(&self) -> DamlCommandService<'_> {
        DamlCommandService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlVersionService`] for querying the participant's
    /// Ledger API version.
    pub fn version_service(&self) -> DamlVersionService<'_> {
        DamlVersionService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlPackageManagementService`] for inspecting
    /// known packages, uploading DARs, validating DARs, and adjusting
    /// the participant's package-vetting topology.
    #[cfg(feature = "admin")]
    pub fn package_management_service(&self) -> DamlPackageManagementService<'_> {
        DamlPackageManagementService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlPartyManagementService`] for inspecting and
    /// administering participant-local party state.
    #[cfg(feature = "admin")]
    pub fn party_management_service(&self) -> DamlPartyManagementService<'_> {
        DamlPartyManagementService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlUserManagementService`] for managing
    /// participant users and their rights.
    #[cfg(feature = "admin")]
    pub fn user_management_service(&self) -> DamlUserManagementService<'_> {
        DamlUserManagementService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlIdentityProviderConfigService`] for managing
    /// runtime-configured Identity Provider configurations.
    #[cfg(feature = "admin")]
    pub fn identity_provider_config_service(&self) -> DamlIdentityProviderConfigService<'_> {
        DamlIdentityProviderConfigService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlCommandInspectionService`] for debugging
    /// in-flight commands on the participant. Alpha; only available
    /// when the participant advertises
    /// `experimental.command_inspection_service.supported` in its
    /// `VersionService.GetLedgerApiVersion` feature descriptor.
    #[cfg(feature = "admin")]
    pub fn command_inspection_service(&self) -> DamlCommandInspectionService<'_> {
        DamlCommandInspectionService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlParticipantPruningService`] for truncating
    /// older portions of the participant-local ledger view.
    #[cfg(feature = "admin")]
    pub fn participant_pruning_service(&self) -> DamlParticipantPruningService<'_> {
        DamlParticipantPruningService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    /// Retrieve a [`DamlTimeService`] for reading and advancing the
    /// participant's static-time clock. Only meaningful when the
    /// participant is configured for static time (see the
    /// `experimental.static_time` flag in
    /// `VersionService::GetLedgerApiVersion`).
    ///
    /// v2 dropped the `sandbox` feature gate — TimeService is part
    /// of every Canton participant's `testing` API; static-vs-
    /// wallclock is a server-side config switch, not a client
    /// build flag.
    pub fn time_service(&self) -> DamlTimeService<'_> {
        DamlTimeService::new(self.channel.clone(), self.config.auth_token.as_deref())
    }

    async fn make_channel(config: &DamlGrpcClientConfig) -> DamlResult<Channel> {
        let mut endpoint = Channel::from_shared(config.uri.clone())?;
        if let Some(limit) = config.concurrency_limit {
            endpoint = endpoint.concurrency_limit(limit);
        }
        if let Some((limit, duration)) = config.rate_limit {
            endpoint = endpoint.rate_limit(limit, duration);
        }
        if let Some(size) = config.initial_stream_window_size {
            endpoint = endpoint.initial_stream_window_size(size);
        }
        if let Some(size) = config.initial_connection_window_size {
            endpoint = endpoint.initial_connection_window_size(size);
        }
        if let Some(duration) = config.tcp_keepalive {
            endpoint = endpoint.tcp_keepalive(Some(duration));
        }
        endpoint = endpoint.tcp_nodelay(config.tcp_nodelay);
        endpoint = endpoint.timeout(config.timeout);
        if let Some(duration) = config.connect_timeout {
            endpoint = endpoint.connect_timeout(duration);
        }
        match &config.tls_config {
            Some(DamlGrpcTlsConfig {
                ca_cert: Some(cert),
            }) => {
                endpoint =
                    endpoint.tls_config(ClientTlsConfig::new().ca_certificate(Certificate::from_pem(cert)))?;
            },
            Some(DamlGrpcTlsConfig {
                ca_cert: None,
            }) => {
                endpoint = endpoint.tls_config(ClientTlsConfig::new())?;
            },
            _ => {},
        }

        endpoint.connect().await.map_err(DamlError::from)
    }

    #[cfg(test)]
    pub(crate) async fn dummy_for_testing() -> Self {
        DamlGrpcClient {
            config: DamlGrpcClientConfig::default(),
            channel: Channel::builder(Uri::from_static("http://dummy.for.testing")).connect_lazy(),
        }
    }
}
