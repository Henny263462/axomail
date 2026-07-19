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

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, Once, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use axomail_core::{CredentialRef, MailError, OAuthConfig};
use axomail_security::OAuthTokens;
use tokio::sync::Mutex as AsyncMutex;

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
/// Refreshes for one account are serialized by a per-account async mutex, so
/// two concurrent syncs cannot both refresh (and, with a rotating refresh
/// token, invalidate each other's stored token). The second caller re-reads
/// the freshly stored token and returns it without a second round-trip.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the account has never signed in or
/// its refresh token is gone (the user must sign in again — a user-actionable
/// class), plus any keychain, network, or authentication error from the
/// refresh.
pub async fn resolve_access_token(
    secrets: &SharedSecretStore,
    reference: &CredentialRef,
    config: &OAuthConfig,
) -> Result<String, MailError> {
    resolve_access_token_with(secrets, reference, |refresh_token| async move {
        refresh_tokens(
            config.token_url.as_str(),
            config.client_id.as_str(),
            &refresh_token,
        )
        .await
    })
    .await
}

/// The refresh-serializing core of [`resolve_access_token`], parameterized over
/// the actual token-endpoint call so it can be exercised against a local stub.
///
/// The per-account lock is held across the load → refresh → store sequence, so
/// a second concurrent caller for the same account blocks, then re-reads the
/// token the first caller just stored (the double-check that avoids a redundant
/// refresh).
async fn resolve_access_token_with<F, Fut>(
    secrets: &SharedSecretStore,
    reference: &CredentialRef,
    refresh: F,
) -> Result<String, MailError>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<OAuthTokens, MailError>>,
{
    let lock = account_refresh_lock(reference);
    let _guard = lock.lock().await;

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

    let refreshed = refresh(tokens.refresh_token).await?;
    store_tokens(secrets, reference, &refreshed).await?;
    Ok(refreshed.access_token)
}

/// Returns the process-wide async mutex guarding refreshes for one account,
/// keyed by its (account-scoped) credential reference. Created on first use.
fn account_refresh_lock(reference: &CredentialRef) -> Arc<AsyncMutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> = OnceLock::new();
    let map = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    // The map guard is a std mutex held only long enough to clone the Arc; the
    // awaited section runs on the per-account async mutex, never this one.
    let mut guard = map.lock().unwrap_or_else(PoisonError::into_inner);
    Arc::clone(
        guard
            .entry(reference.as_str().to_owned())
            .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
    )
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
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use axomail_security::InMemorySecretStore;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn store() -> SharedSecretStore {
        Arc::new(InMemorySecretStore::new())
    }

    /// A local token endpoint that counts the refreshes it serves and always
    /// returns a *rotated* refresh token, so a caller can prove exactly one
    /// refresh happened and that the new set replaced the old one. Loops so it
    /// can (but here must not) serve more than one request.
    async fn counting_token_endpoint() -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_task = Arc::clone(&hits);
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                hits_task.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let body = r#"{"access_token":"fresh-access","refresh_token":"rotated-refresh","expires_in":3600}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });
        (url, hits)
    }

    /// Stores an already-expired token set under `reference`.
    async fn store_expired(secrets: &SharedSecretStore, reference: &CredentialRef) {
        let tokens = OAuthTokens {
            access_token: "stale".to_owned(),
            refresh_token: "r1".to_owned(),
            expires_at_unix: unix_now() - 10,
        };
        store_tokens(secrets, reference, &tokens).await.unwrap();
    }

    #[tokio::test]
    async fn refresh_rotates_the_stored_token_set_with_a_single_write() {
        let secrets = store();
        // A distinct reference so the process-wide lock map cannot collide with
        // other tests in this binary.
        let reference = CredentialRef::new("account/rotation/oauth");
        store_expired(&secrets, &reference).await;
        let (url, hits) = counting_token_endpoint().await;

        let access = resolve_access_token_with(&secrets, &reference, |refresh_token| {
            let url = url.clone();
            async move { refresh_tokens(&url, "client", &refresh_token).await }
        })
        .await
        .unwrap();
        assert_eq!(access, "fresh-access");

        // The single stored set both directions read now carries the rotated
        // refresh token; the old "r1" is gone.
        let stored = load_tokens(&secrets, &reference).await.unwrap().unwrap();
        assert_eq!(stored.access_token, "fresh-access");
        assert_eq!(stored.refresh_token, "rotated-refresh");
        assert_eq!(hits.load(Ordering::SeqCst), 1, "exactly one refresh write");

        // The "other direction" resolving the same reference sees the fresh
        // token without another endpoint round-trip.
        let again = resolve_access_token_with(&secrets, &reference, |refresh_token| {
            let url = url.clone();
            async move { refresh_tokens(&url, "client", &refresh_token).await }
        })
        .await
        .unwrap();
        assert_eq!(again, "fresh-access");
        assert_eq!(hits.load(Ordering::SeqCst), 1, "no second refresh");
    }

    #[tokio::test]
    async fn concurrent_resolves_serialize_into_one_refresh() {
        let secrets = store();
        let reference = CredentialRef::new("account/concurrent/oauth");
        store_expired(&secrets, &reference).await;
        let (url, hits) = counting_token_endpoint().await;

        let first = resolve_access_token_with(&secrets, &reference, |refresh_token| {
            let url = url.clone();
            async move { refresh_tokens(&url, "client", &refresh_token).await }
        });
        let second = resolve_access_token_with(&secrets, &reference, |refresh_token| {
            let url = url.clone();
            async move { refresh_tokens(&url, "client", &refresh_token).await }
        });
        let (a, b) = tokio::join!(first, second);

        assert_eq!(a.unwrap(), "fresh-access");
        assert_eq!(b.unwrap(), "fresh-access");
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "the per-account lock collapses the two resolves into one refresh"
        );
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
