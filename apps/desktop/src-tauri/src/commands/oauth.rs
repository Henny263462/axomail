//! The `oauth_authorize` command: run the RFC 8252 browser sign-in for an
//! existing OAuth2 account and store the resulting tokens.
//!
//! The account row (with its non-secret OAuth endpoints) is created first by
//! `create_account`; this command performs the interactive sign-in and writes
//! the access/refresh tokens into the keychain under the account's existing
//! credential references — the same references a password account would use.
//!
//! Only a status crosses back to the UI; tokens never do. The system browser
//! is opened through the opener plugin (scoped to `https` in the capability
//! set), and the loopback receiver only ever binds `127.0.0.1`.

use std::time::Duration;

use axomail_core::{AccountAuth, AccountId, MailError};
use axomail_security::credential_ref_for_oauth;
use axomail_storage::Store;
use tauri_plugin_opener::OpenerExt;

use crate::commands::map_command_error;
use crate::dto::{OAuthAuthorizeResult, OAuthAuthorizeStatus};
use crate::oauth::pkce::{Pkce, random_state};
use crate::oauth::{self, LoopbackResult, LoopbackServer};
use crate::state::{AppState, SharedSecretStore};

/// How long to wait for the user to complete the browser consent.
const AUTH_TIMEOUT: Duration = Duration::from_secs(300);

/// Outcome of the browser flow, independent of the DTO representation.
pub enum OAuthOutcome {
    /// Consent granted; tokens stored.
    Authorized,
    /// The user declined.
    Denied,
    /// No redirect arrived before the deadline.
    TimedOut,
}

/// Drives the full sign-in: build the consent URL, open the browser via
/// `open_browser`, receive the loopback redirect, exchange the code, and store
/// the token set under the account's single shared OAuth credential reference.
///
/// `open_browser` is injected so the flow is testable without a real browser
/// (a test drives the loopback directly).
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the account is missing or not an
/// OAuth2 account, plus any error from opening the browser, the loopback
/// server (other than timeout, which is a normal outcome), the token exchange,
/// or the keychain write.
pub async fn run_browser_flow<F>(
    store: &Store,
    secrets: &SharedSecretStore,
    account_id: i64,
    timeout: Duration,
    open_browser: F,
) -> Result<OAuthOutcome, MailError>
where
    F: FnOnce(&str) -> Result<(), MailError>,
{
    let account = store
        .accounts()
        .get(AccountId::new(account_id))
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "no such account".to_owned(),
        })?;
    let AccountAuth::OAuth2(config) = &account.auth else {
        return Err(MailError::Configuration {
            message: "this account does not use OAuth2".to_owned(),
        });
    };

    let pkce = Pkce::generate()?;
    let state = random_state()?;
    let server = LoopbackServer::bind().await?;
    let redirect_uri = server.redirect_uri().to_owned();
    let url = oauth::build_authorize_url(config, &redirect_uri, &state, &pkce.challenge);
    open_browser(&url)?;

    match server.wait_for_redirect(&state, timeout).await {
        Ok(LoopbackResult::Code(code)) => {
            let tokens = oauth::exchange_code(
                config.token_url.as_str(),
                config.client_id.as_str(),
                &code,
                &redirect_uri,
                &pkce.verifier,
            )
            .await?;
            // One token set per account serves both the incoming and outgoing
            // connections, stored under the single account-scoped OAuth
            // reference. A later refresh (with token rotation) rewrites this one
            // entry, so no direction can be left holding an invalidated token.
            oauth::store_tokens(secrets, &credential_ref_for_oauth(account.id), &tokens).await?;
            Ok(OAuthOutcome::Authorized)
        }
        Ok(LoopbackResult::Denied) => Ok(OAuthOutcome::Denied),
        Err(MailError::Timeout { .. }) => Ok(OAuthOutcome::TimedOut),
        Err(other) => Err(other),
    }
}

/// Maps the flow outcome to its DTO.
fn to_result(outcome: OAuthOutcome) -> OAuthAuthorizeResult {
    OAuthAuthorizeResult::new(match outcome {
        OAuthOutcome::Authorized => OAuthAuthorizeStatus::Authorized,
        OAuthOutcome::Denied => OAuthAuthorizeStatus::Denied,
        OAuthOutcome::TimedOut => OAuthAuthorizeStatus::TimedOut,
    })
}

/// Runs the browser sign-in for an OAuth2 account and stores its tokens.
#[tauri::command]
pub async fn oauth_authorize(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    account_id: i64,
) -> Result<OAuthAuthorizeResult, String> {
    let store = state.store().clone();
    let secrets = state.secrets().clone();
    let opener = app.clone();
    let outcome = run_browser_flow(&store, &secrets, account_id, AUTH_TIMEOUT, move |url| {
        opener
            .opener()
            .open_url(url.to_owned(), None::<String>)
            .map_err(|_| MailError::Network {
                message: "could not open the system browser for sign-in".to_owned(),
            })
    })
    .await
    .map_err(map_command_error)?;
    Ok(to_result(outcome))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use axomail_core::{
        Account, ConnectionSecurity, CredentialRef, EmailAddress, HttpsUrl, IncomingProtocol,
        IncomingServerConfig, OAuthClientId, OAuthConfig, OAuthScopes, OutgoingServerConfig,
        ServerEndpoint,
    };
    use axomail_security::InMemorySecretStore;
    use axomail_storage::NewAccount;
    use std::num::NonZeroU16;
    use std::sync::Arc;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpStream;

    async fn store_with_oauth_account() -> (Store, i64) {
        let store = Store::open_in_memory().await.unwrap();
        let port = |value: u16| NonZeroU16::new(value).unwrap();
        let new = NewAccount {
            display_name: "Gmail".to_owned(),
            email: EmailAddress::parse("k@gmail.com").unwrap(),
            auth: AccountAuth::OAuth2(OAuthConfig {
                client_id: OAuthClientId::parse("cid").unwrap(),
                auth_url: HttpsUrl::parse("https://auth.example.org/a").unwrap(),
                token_url: HttpsUrl::parse("https://auth.example.org/t").unwrap(),
                scopes: OAuthScopes::new(vec!["scope".to_owned()]).unwrap(),
            }),
            incoming: IncomingServerConfig {
                protocol: IncomingProtocol::Imap,
                endpoint: ServerEndpoint::new("imap.gmail.com", port(993)).unwrap(),
                security: ConnectionSecurity::Tls,
                username: "k@gmail.com".to_owned(),
                credential: CredentialRef::new("account/1/incoming"),
            },
            outgoing: OutgoingServerConfig {
                endpoint: ServerEndpoint::new("smtp.gmail.com", port(465)).unwrap(),
                security: ConnectionSecurity::Tls,
                username: "k@gmail.com".to_owned(),
                credential: CredentialRef::new("account/1/outgoing"),
            },
        };
        let account: Account = store.accounts().insert(&new).await.unwrap();
        (store, account.id.value())
    }

    fn secrets() -> SharedSecretStore {
        Arc::new(InMemorySecretStore::new())
    }

    /// Extracts `redirect_uri` and `state` from a generated authorize URL and
    /// drives the loopback with the given query (`error`/`code`), standing in
    /// for the browser.
    fn drive(url: &str, query: String) {
        let redirect = url
            .split("redirect_uri=")
            .nth(1)
            .and_then(|rest| rest.split('&').next())
            .unwrap()
            .replace("%3A", ":")
            .replace("%2F", "/");
        let state = url
            .split("state=")
            .nth(1)
            .and_then(|rest| rest.split('&').next())
            .unwrap()
            .to_owned();
        let addr = redirect.strip_prefix("http://").unwrap().to_owned();
        tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            let request = format!("GET /?{query}&state={state} HTTP/1.1\r\nHost: x\r\n\r\n");
            let _ = stream.write_all(request.as_bytes()).await;
        });
    }

    #[tokio::test]
    async fn missing_account_is_configuration_error() {
        let store = Store::open_in_memory().await.unwrap();
        let result = run_browser_flow(&store, &secrets(), 999, AUTH_TIMEOUT, |_| Ok(())).await;
        assert!(matches!(result, Err(MailError::Configuration { .. })));
    }

    #[tokio::test]
    async fn password_account_is_rejected() {
        let store = Store::open_in_memory().await.unwrap();
        let new = NewAccount {
            display_name: "Pw".to_owned(),
            email: EmailAddress::parse("k@example.org").unwrap(),
            auth: AccountAuth::Password,
            incoming: IncomingServerConfig {
                protocol: IncomingProtocol::Imap,
                endpoint: ServerEndpoint::new("imap.example.org", NonZeroU16::new(993).unwrap())
                    .unwrap(),
                security: ConnectionSecurity::Tls,
                username: "k@example.org".to_owned(),
                credential: CredentialRef::new("account/1/incoming"),
            },
            outgoing: OutgoingServerConfig {
                endpoint: ServerEndpoint::new("smtp.example.org", NonZeroU16::new(465).unwrap())
                    .unwrap(),
                security: ConnectionSecurity::Tls,
                username: "k@example.org".to_owned(),
                credential: CredentialRef::new("account/1/outgoing"),
            },
        };
        let id = store.accounts().insert(&new).await.unwrap().id.value();
        let result = run_browser_flow(&store, &secrets(), id, AUTH_TIMEOUT, |_| Ok(())).await;
        assert!(matches!(result, Err(MailError::Configuration { .. })));
    }

    #[tokio::test]
    async fn user_denial_yields_denied_outcome() {
        let (store, id) = store_with_oauth_account().await;
        let outcome = run_browser_flow(&store, &secrets(), id, Duration::from_secs(5), |url| {
            drive(url, "error=access_denied".to_owned());
            Ok(())
        })
        .await
        .unwrap();
        assert!(matches!(outcome, OAuthOutcome::Denied));
    }

    #[tokio::test]
    async fn no_redirect_times_out() {
        let (store, id) = store_with_oauth_account().await;
        let outcome =
            run_browser_flow(
                &store,
                &secrets(),
                id,
                Duration::from_millis(80),
                |_| Ok(()),
            )
            .await
            .unwrap();
        assert!(matches!(outcome, OAuthOutcome::TimedOut));
    }

    #[tokio::test]
    async fn failure_to_open_browser_propagates() {
        let (store, id) = store_with_oauth_account().await;
        let result = run_browser_flow(&store, &secrets(), id, AUTH_TIMEOUT, |_| {
            Err(MailError::Network {
                message: "no browser".to_owned(),
            })
        })
        .await;
        assert!(matches!(result, Err(MailError::Network { .. })));
    }
}
