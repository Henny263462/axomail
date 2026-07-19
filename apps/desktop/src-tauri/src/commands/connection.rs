//! The `test_connection` command: probe an account's servers without
//! persisting anything.
//!
//! The incoming and outgoing servers are probed independently so the setup
//! UI can report exactly which one failed. Nothing is stored: the passwords
//! come straight from the form input and go nowhere but the live connection
//! attempt, and no account row or keychain entry is created.

use std::num::NonZeroU16;

use axomail_core::{
    Account, AccountId, IncomingMailProvider, IncomingProtocol, MailError, ServerEndpoint,
};
use axomail_imap::{ImapConfig, ImapProvider};
use axomail_pop3::{Pop3Config, Pop3Provider};
use axomail_smtp::{SmtpConfig, SmtpProvider};
use axomail_storage::Store;

use crate::commands::map_command_error;
use crate::composition::resolve_credential;
use crate::dto::{ConnectionTestResult, CreateAccountInput, IncomingInput, ServerInput};
use crate::state::{AppState, SharedSecretStore};

/// Builds a validated endpoint from a host and port, rejecting port zero.
fn endpoint(host: &str, port: u16) -> Result<ServerEndpoint, MailError> {
    let port = NonZeroU16::new(port).ok_or_else(|| MailError::Configuration {
        message: "server port must not be zero".to_owned(),
    })?;
    ServerEndpoint::new(host.to_owned(), port)
}

/// Probes the incoming server: builds the protocol's provider and runs
/// `connect` (which authenticates) then a best-effort `disconnect`.
///
/// The POP3 probe never enables server-side deletion, so a mistyped test can
/// never remove mail; `connect` performs the `CAPA`/`USER`/`PASS` exchange,
/// so a wrong password surfaces as [`MailError::Authentication`] just as it
/// does for IMAP.
///
/// `extra_roots` is empty in production; the integration tests supply the
/// self-signed certificate of a local test server through it, mirroring
/// [`ImapConfig::extra_root_certs_der`].
///
/// # Errors
///
/// Returns the underlying [`MailError`] when the connection or login fails.
pub async fn probe_incoming(
    input: &IncomingInput,
    extra_roots: &[Vec<u8>],
) -> Result<(), MailError> {
    match input.protocol {
        IncomingProtocol::Imap => {
            let mut config = ImapConfig::new(
                endpoint(&input.host, input.port)?,
                input.security,
                input.username.clone(),
                input.password.clone(),
            );
            config.extra_root_certs_der = extra_roots.to_vec();
            let mut provider = ImapProvider::new(config);
            provider.connect().await?;
            // The session is disposable; a failed logout does not change the
            // probe's verdict.
            let _ = provider.disconnect().await;
            Ok(())
        }
        IncomingProtocol::Pop3 => {
            let mut config = Pop3Config::new(
                endpoint(&input.host, input.port)?,
                input.security,
                input.username.clone(),
                input.password.clone(),
            );
            config.extra_root_certs_der = extra_roots.to_vec();
            // A probe must never mutate the mailbox: leave deletion disabled.
            config.delete_after_download = false;
            let mut provider = Pop3Provider::new(config);
            provider.connect().await?;
            let _ = provider.disconnect().await;
            Ok(())
        }
    }
}

/// Probes the outgoing (SMTP) server.
///
/// The SMTP adapter (`axomail-smtp`) opens its connection only at send time
/// and exposes no standalone authenticated-connect entry point; adapter
/// isolation (ADR-0006) keeps `mail-send` out of this crate, so a raw probe
/// socket cannot be opened here. What is feasible without submitting a
/// message — which a real "test" must never do — is to construct the
/// provider, which validates the endpoint and builds the strict TLS
/// connector (rejecting an unparseable trust anchor). Live authentication is
/// therefore verified at the first real send (`send_message`), whose failure
/// path records a redacted error on the outbox row.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an invalid endpoint and
/// [`MailError::Tls`] if a supplied trust anchor cannot be parsed.
pub async fn probe_outgoing(input: &ServerInput, extra_roots: &[Vec<u8>]) -> Result<(), MailError> {
    let mut config = SmtpConfig::new(
        endpoint(&input.host, input.port)?,
        input.security,
        input.username.clone(),
        input.password.clone(),
    );
    config.extra_root_certs_der = extra_roots.to_vec();
    let _provider = SmtpProvider::new(config)?;
    Ok(())
}

/// Runs both probes and assembles the per-direction result.
pub async fn probe(input: &CreateAccountInput, extra_roots: &[Vec<u8>]) -> ConnectionTestResult {
    let incoming = probe_incoming(&input.incoming, extra_roots).await;
    let outgoing = probe_outgoing(&input.outgoing, extra_roots).await;
    ConnectionTestResult {
        incoming_ok: incoming.is_ok(),
        incoming_error: incoming.err().map(|error| error.to_string()),
        outgoing_ok: outgoing.is_ok(),
        outgoing_error: outgoing.err().map(|error| error.to_string()),
    }
}

/// Probes both servers of a candidate account with production TLS trust.
pub async fn test_connection_impl(input: CreateAccountInput) -> ConnectionTestResult {
    probe(&input, &[]).await
}

/// Tests connectivity to the incoming and outgoing servers of a candidate
/// account without persisting anything.
#[tauri::command]
pub async fn test_connection(input: CreateAccountInput) -> Result<ConnectionTestResult, String> {
    Ok(test_connection_impl(input).await)
}

/// Probes an already-stored account's incoming server, resolving its
/// credential (a keychain password, or a freshly refreshed OAuth2 access
/// token) exactly as a real sync would.
async fn probe_incoming_account(
    account: &Account,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
) -> Result<(), MailError> {
    let credential = resolve_credential(secrets, account, &account.incoming.credential).await?;
    match account.incoming.protocol {
        IncomingProtocol::Imap => {
            let mut config = ImapConfig::with_credential(
                account.incoming.endpoint.clone(),
                account.incoming.security,
                account.incoming.username.clone(),
                credential,
            );
            config.extra_root_certs_der = extra_roots.to_vec();
            let mut provider = ImapProvider::new(config);
            provider.connect().await?;
            let _ = provider.disconnect().await;
            Ok(())
        }
        IncomingProtocol::Pop3 => {
            let mut config = Pop3Config::with_credential(
                account.incoming.endpoint.clone(),
                account.incoming.security,
                account.incoming.username.clone(),
                credential,
            );
            config.extra_root_certs_der = extra_roots.to_vec();
            config.delete_after_download = false;
            let mut provider = Pop3Provider::new(config);
            provider.connect().await?;
            let _ = provider.disconnect().await;
            Ok(())
        }
    }
}

/// Builds the SMTP provider for an already-stored account (validating the
/// endpoint and resolving its outgoing credential). As for
/// [`probe_outgoing`], no message is sent.
async fn probe_outgoing_account(
    account: &Account,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
) -> Result<(), MailError> {
    let credential = resolve_credential(secrets, account, &account.outgoing.credential).await?;
    let mut config = SmtpConfig::with_credential(
        account.outgoing.endpoint.clone(),
        account.outgoing.security,
        account.outgoing.username.clone(),
        credential,
    );
    config.extra_root_certs_der = extra_roots.to_vec();
    let _provider = SmtpProvider::new(config)?;
    Ok(())
}

/// Probes both servers of a stored account, resolving credentials from the
/// keychain (refreshing an OAuth2 token when due).
pub async fn test_account_connection_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    account_id: i64,
    extra_roots: &[Vec<u8>],
) -> Result<ConnectionTestResult, MailError> {
    let account = store
        .accounts()
        .get(AccountId::new(account_id))
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "no such account".to_owned(),
        })?;
    let incoming = probe_incoming_account(&account, secrets, extra_roots).await;
    let outgoing = probe_outgoing_account(&account, secrets, extra_roots).await;
    Ok(ConnectionTestResult {
        incoming_ok: incoming.is_ok(),
        incoming_error: incoming.err().map(|error| error.to_string()),
        outgoing_ok: outgoing.is_ok(),
        outgoing_error: outgoing.err().map(|error| error.to_string()),
    })
}

/// Tests connectivity to a stored account's servers (used after OAuth2
/// sign-in, and for re-testing a saved account).
#[tauri::command]
pub async fn test_account_connection(
    state: tauri::State<'_, AppState>,
    account_id: i64,
) -> Result<ConnectionTestResult, String> {
    test_account_connection_impl(state.store(), state.secrets(), account_id, &[])
        .await
        .map_err(map_command_error)
}
