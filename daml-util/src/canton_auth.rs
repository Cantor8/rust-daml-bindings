//! JWT token builder for **Canton v2** participants.
//!
//! v0.2's `DamlSandboxTokenBuilder` was built around the v1
//! `https://daml.com/ledger-api` custom claim shape; Canton v2
//! switched to audience-scoped tokens whose contents follow the
//! standard `aud`/`sub`/`scope` claim layout. This module is the
//! v2 replacement.
//!
//! # Claim shape
//!
//! A Canton v2 token typically looks like:
//!
//! ```json
//! {
//!   "aud": "https://daml.com/jwt/aud/participant/<participant-id>",
//!   "sub": "<user-id>",
//!   "iss": "<issuer>",
//!   "scope": "daml_ledger_api",
//!   "exp": 1700000000,
//!   "iat": 1700000000
//! }
//! ```
//!
//! * `aud` — the participant's expected audience URL (configured
//!   on the participant side under `auth-services`). For dev
//!   sandboxes this is often left wildcard or empty.
//! * `sub` — the Daml ledger user-id the token authorises. The
//!   user must already have been registered via
//!   `UserManagementService.CreateUser` for the participant to
//!   honour the token's `act_as` / `read_as` rights.
//! * `scope` — must contain `daml_ledger_api` for ledger API
//!   access. Multiple scopes are space-separated per RFC 6749.
//! * `iss`, `iat`, `exp` — standard JWT timestamps.
//!
//! # Examples
//!
//! ```
//! # use daml_util::DamlCantonTokenResult;
//! # fn main() -> DamlCantonTokenResult<()> {
//! use daml_util::DamlCantonTokenBuilder;
//!
//! let token = DamlCantonTokenBuilder::new_with_duration_secs(60)
//!     .audience("https://daml.com/jwt/aud/participant/sandbox")
//!     .subject("alice")
//!     .scope("daml_ledger_api")
//!     .new_hs256_unsafe_token("dev-shared-secret")?;
//! # let _ = token;
//! # Ok(())
//! # }
//! ```
//!
//! The `_unsafe` suffix on `new_hs256_unsafe_token` is intentional:
//! HS256 with a shared secret is fine for local sandboxes, but
//! production deployments should use an asymmetric key (RS256 / ES256)
//! managed by an OIDC IdP so the participant doesn't need to share
//! signing material with token-minting code.

use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DamlCantonTokenError {
    #[error("JWT signing failed: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),
}

pub type DamlCantonTokenResult<T> = std::result::Result<T, DamlCantonTokenError>;

/// Standard-shaped JWT claims emitted by [`DamlCantonTokenBuilder`].
///
/// All fields are `Option<...>` except `exp` so absent values
/// don't get serialised as empty strings (which the participant
/// would reject as a claim-format violation).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DamlCantonClaims {
    /// Issuer URL — typically the OIDC IdP. Optional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// Subject — the Daml user-id the token authorises.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    /// Audience — the participant's `aud` URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// Space-separated scope list; must contain `daml_ledger_api`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Issued-at (seconds since epoch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<i64>,
    /// Expiry (seconds since epoch). Required by the participant.
    pub exp: i64,
}

/// Build a Canton v2 JWT.
///
/// Configure the claims first via the builder setters, then sign
/// with one of `new_hs256_unsafe_token`, `new_rs256_token`, or
/// `new_es256_token`.
#[derive(Debug, Clone)]
pub struct DamlCantonTokenBuilder {
    claims: DamlCantonClaims,
}

impl DamlCantonTokenBuilder {
    /// Construct a builder with a TTL in seconds (relative to the
    /// current wall clock). Sets `iat` and `exp` accordingly.
    pub fn new_with_duration_secs(duration_secs: i64) -> Self {
        let now = Utc::now();
        let exp = (now + Duration::seconds(duration_secs)).timestamp();
        Self {
            claims: DamlCantonClaims {
                iat: Some(now.timestamp()),
                exp,
                ..DamlCantonClaims::default()
            },
        }
    }

    /// Construct a builder with an absolute expiry (seconds since
    /// epoch). Useful when minting a token that must align with an
    /// external session boundary.
    pub fn new_with_expiry(expiry_epoch_secs: i64) -> Self {
        Self {
            claims: DamlCantonClaims {
                iat: Some(Utc::now().timestamp()),
                exp: expiry_epoch_secs,
                ..DamlCantonClaims::default()
            },
        }
    }

    pub fn issuer(mut self, iss: impl Into<String>) -> Self {
        self.claims.iss = Some(iss.into());
        self
    }

    pub fn subject(mut self, sub: impl Into<String>) -> Self {
        self.claims.sub = Some(sub.into());
        self
    }

    pub fn audience(mut self, aud: impl Into<String>) -> Self {
        self.claims.aud = Some(aud.into());
        self
    }

    pub fn scope(mut self, scope: impl Into<String>) -> Self {
        self.claims.scope = Some(scope.into());
        self
    }

    pub fn claims(&self) -> &DamlCantonClaims {
        &self.claims
    }

    /// Sign with **HMAC-SHA256** using a shared secret.
    ///
    /// The `_unsafe` suffix is intentional: shared secrets are
    /// fine for local sandbox testing but generally inappropriate
    /// for production because every party that needs to mint a
    /// token must also be able to forge anyone else's token.
    pub fn new_hs256_unsafe_token(self, secret: impl AsRef<[u8]>) -> DamlCantonTokenResult<String> {
        let header = Header::new(Algorithm::HS256);
        let key = EncodingKey::from_secret(secret.as_ref());
        Ok(jsonwebtoken::encode(&header, &self.claims, &key)?)
    }

    /// Sign with **RS256** using a PEM-encoded private RSA key.
    pub fn new_rs256_token(self, pem: impl AsRef<[u8]>) -> DamlCantonTokenResult<String> {
        let header = Header::new(Algorithm::RS256);
        let key = EncodingKey::from_rsa_pem(pem.as_ref())?;
        Ok(jsonwebtoken::encode(&header, &self.claims, &key)?)
    }

    /// Sign with **ES256** using a PEM-encoded private EC key.
    pub fn new_es256_token(self, pem: impl AsRef<[u8]>) -> DamlCantonTokenResult<String> {
        let header = Header::new(Algorithm::ES256);
        let key = EncodingKey::from_ec_pem(pem.as_ref())?;
        Ok(jsonwebtoken::encode(&header, &self.claims, &key)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{DecodingKey, Validation, decode};

    #[test]
    fn hs256_roundtrip() {
        let secret = "test-secret";
        let token = DamlCantonTokenBuilder::new_with_duration_secs(60)
            .issuer("https://example.invalid")
            .subject("alice")
            .audience("https://daml.com/jwt/aud/participant/sandbox")
            .scope("daml_ledger_api")
            .new_hs256_unsafe_token(secret)
            .expect("sign");
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_audience(&["https://daml.com/jwt/aud/participant/sandbox"]);
        let decoded = decode::<DamlCantonClaims>(&token, &DecodingKey::from_secret(secret.as_ref()), &validation)
            .expect("decode");
        let claims = decoded.claims;
        assert_eq!(claims.sub.as_deref(), Some("alice"));
        assert_eq!(claims.scope.as_deref(), Some("daml_ledger_api"));
        assert_eq!(claims.iss.as_deref(), Some("https://example.invalid"));
    }

    #[test]
    fn expiry_is_in_the_future() {
        let builder = DamlCantonTokenBuilder::new_with_duration_secs(60);
        let now = Utc::now().timestamp();
        assert!(builder.claims().exp >= now);
        assert!(builder.claims().exp <= now + 60 + 1);
    }
}
