//! OAuth2 / XOAUTH2 engine (ADR-0009).
//!
//! Pure protocol mechanics live here; the Tauri command layer
//! ([`crate::commands::oauth`]) orchestrates them and owns the system-browser
//! step. The pieces are:
//!
//! - [`pkce`]: PKCE verifier/challenge and the CSRF `state`.
//! - [`build_authorize_url`]: the consent URL.
//! - [`LoopbackServer`]: the one-shot RFC 8252 loopback redirect receiver.
//! - [`exchange_code`] / [`refresh_tokens`]: the token endpoint.
//! - [`resolve_access_token`]: load tokens from the keychain, refresh when due
//!   (with a clock-skew margin), persist a rotated refresh token, and hand a
//!   fresh access token to the connection layer.
//!
//! Access and refresh tokens are secrets and live only in the OS keychain via
//! [`OAuthTokens`]; nothing here logs or returns them.

pub mod pkce;

mod loopback;
mod token;
mod url;

use std::sync::{Arc, Once};
use std::time::{SystemTime, UNIX_EPOCH};

use axomail_core::{CredentialRef, MailError, OAuthConfig};
use axomail_security::OAuthTokens;

use crate::state::SharedSecretStore;

pub use loopback::{LoopbackResult, LoopbackServer};
pub use token::{exchange_code, refresh_tokens};
pub use url::build_authorize_url;

/// Margin before real expiry at which an access token is proactively
/// refreshed, so a token never expires mid-request.
const REFRESH_SKEW_SECS: i64 = 120;

/// Installs the process-wide rustls crypto provider (ring) exactly once.
///
/// reqwest is built with `rustls-no-provider` (ADR-0009), so a default
/// provider must be installed before any TLS client is constructed. This is
/// idempotent and order-independent: a second call, or a call after another
/// component already installed a provider, is a harmless no-op.
pub fn install_crypto_provider() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        // Ignore the result: an `Err` means a provider is already installed,
        // which is exactly the state we want.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Resolves a usable OAuth2 access token for a connection, refreshing it when
/// it is expired or within the skew window and persisting any rotated refresh
/// token back to the keychain.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the account has never signed in or
/// its refresh token is gone (the user must sign in again), plus any
/// keychain, network, or authentication error from the refresh.
pub async fn resolve_access_token(
    secrets: &SharedSecretStore,
    reference: &CredentialRef,
    config: &OAuthConfig,
) -> Result<String, MailError> {
    let tokens =
        load_tokens(secrets, reference)
            .await?
            .ok_or_else(|| MailError::Configuration {
                message: "this account is not signed in yet; use \"Sign in with browser\""
                    .to_owned(),
            })?;

    if !tokens.is_expired(unix_now(), REFRESH_SKEW_SECS) {
        return Ok(tokens.access_token);
    }
    if tokens.refresh_token.is_empty() {
        return Err(MailError::Configuration {
            message: "the sign-in for this account has expired; sign in again".to_owned(),
        });
    }

    let refreshed = refresh_tokens(
        config.token_url.as_str(),
        config.client_id.as_str(),
        &tokens.refresh_token,
    )
    .await?;
    store_tokens(secrets, reference, &refreshed).await?;
    Ok(refreshed.access_token)
}

/// Persists a token set as JSON under `reference` on a blocking task.
///
/// # Errors
///
/// Returns [`MailError::Keychain`] if the write fails.
pub async fn store_tokens(
    secrets: &SharedSecretStore,
    reference: &CredentialRef,
    tokens: &OAuthTokens,
) -> Result<(), MailError> {
    let json = tokens.to_json()?;
    let secrets = Arc::clone(secrets);
    let reference = reference.clone();
    match tokio::task::spawn_blocking(move || secrets.set_secret(&reference, &json)).await {
        Ok(result) => result,
        Err(_) => Err(MailError::Keychain {
            message: "keychain write did not complete".to_owned(),
        }),
    }
}

/// Loads and parses the token set stored under `reference`, if any.
///
/// # Errors
///
/// Returns [`MailError::Keychain`] on a read failure or an unparseable
/// payload.
pub async fn load_tokens(
    secrets: &SharedSecretStore,
    reference: &CredentialRef,
) -> Result<Option<OAuthTokens>, MailError> {
    let secrets = Arc::clone(secrets);
    let reference = reference.clone();
    let raw = match tokio::task::spawn_blocking(move || secrets.get_secret(&reference)).await {
        Ok(result) => result?,
        Err(_) => {
            return Err(MailError::Keychain {
                message: "keychain read did not complete".to_owned(),
            });
        }
    };
    raw.map(|payload| OAuthTokens::from_json(&payload))
        .transpose()
}

/// Current Unix time in seconds; a clock before the epoch yields 0.
pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|delta| i64::try_from(delta.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use axomail_security::InMemorySecretStore;

    fn store() -> SharedSecretStore {
        Arc::new(InMemorySecretStore::new())
    }

    #[tokio::test]
    async fn load_returns_none_when_unset() {
        let secrets = store();
        let reference = CredentialRef::new("account/1/incoming");
        assert!(load_tokens(&secrets, &reference).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn store_then_load_round_trips() {
        let secrets = store();
        let reference = CredentialRef::new("account/1/incoming");
        let tokens = OAuthTokens {
            access_token: "acc".to_owned(),
            refresh_token: "ref".to_owned(),
            expires_at_unix: unix_now() + 3600,
        };
        store_tokens(&secrets, &reference, &tokens).await.unwrap();
        let loaded = load_tokens(&secrets, &reference).await.unwrap().unwrap();
        assert_eq!(loaded, tokens);
    }

    #[tokio::test]
    async fn resolve_returns_stored_token_when_fresh() {
        let secrets = store();
        let reference = CredentialRef::new("account/1/incoming");
        let config = OAuthConfig {
            client_id: axomail_core::OAuthClientId::parse("c").unwrap(),
            auth_url: axomail_core::HttpsUrl::parse("https://a.example.org").unwrap(),
            token_url: axomail_core::HttpsUrl::parse("https://t.example.org").unwrap(),
            scopes: axomail_core::OAuthScopes::new(vec!["s".to_owned()]).unwrap(),
        };
        let tokens = OAuthTokens {
            access_token: "still-good".to_owned(),
            refresh_token: "ref".to_owned(),
            expires_at_unix: unix_now() + 3600,
        };
        store_tokens(&secrets, &reference, &tokens).await.unwrap();
        let access = resolve_access_token(&secrets, &reference, &config)
            .await
            .unwrap();
        assert_eq!(access, "still-good");
    }

    #[tokio::test]
    async fn resolve_errors_when_not_signed_in() {
        let secrets = store();
        let reference = CredentialRef::new("account/1/incoming");
        let config = OAuthConfig {
            client_id: axomail_core::OAuthClientId::parse("c").unwrap(),
            auth_url: axomail_core::HttpsUrl::parse("https://a.example.org").unwrap(),
            token_url: axomail_core::HttpsUrl::parse("https://t.example.org").unwrap(),
            scopes: axomail_core::OAuthScopes::new(vec!["s".to_owned()]).unwrap(),
        };
        let err = resolve_access_token(&secrets, &reference, &config)
            .await
            .unwrap_err();
        assert!(matches!(err, MailError::Configuration { .. }));
    }
}
