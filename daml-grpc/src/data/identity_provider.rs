use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::IdentityProviderConfig;

/// Configuration of an Identity Provider (IDP) on the participant.
///
/// The participant uses the JWT `iss` claim to route an access token to
/// the matching IDP. Tokens that don't match any configured IDP fall
/// through to the default IDP that is fixed at participant deployment.
/// Users and parties carry an `identity_provider_id` that scopes which
/// IDP admin can manage them.
///
/// Modifiable via `UpdateIdentityProviderConfig` (FieldMask-driven):
/// `is_deactivated`, `issuer`, `jwks_url`, `audience`. The
/// `identity_provider_id` itself is the primary key and cannot change.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct DamlIdentityProviderConfig {
    /// Stable IDP id. Required, not modifiable.
    pub identity_provider_id: String,
    /// When `true`, every token issued by this IDP is rejected.
    pub is_deactivated: bool,
    /// Expected JWT `iss` claim — the HTTPS URL of the IDP. Used by the
    /// participant to match incoming tokens to this configuration.
    pub issuer: String,
    /// URL of the IDP's JWKS endpoint. The participant fetches
    /// RS256-only signing keys from here to verify token signatures.
    pub jwks_url: String,
    /// Expected JWT `aud` claim. Empty disables audience checking.
    pub audience: String,
}

impl From<IdentityProviderConfig> for DamlIdentityProviderConfig {
    fn from(c: IdentityProviderConfig) -> Self {
        Self {
            identity_provider_id: c.identity_provider_id,
            is_deactivated: c.is_deactivated,
            issuer: c.issuer,
            jwks_url: c.jwks_url,
            audience: c.audience,
        }
    }
}

impl From<DamlIdentityProviderConfig> for IdentityProviderConfig {
    fn from(c: DamlIdentityProviderConfig) -> Self {
        Self {
            identity_provider_id: c.identity_provider_id,
            is_deactivated: c.is_deactivated,
            issuer: c.issuer,
            jwks_url: c.jwks_url,
            audience: c.audience,
        }
    }
}
