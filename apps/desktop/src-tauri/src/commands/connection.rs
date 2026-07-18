//! The `test_connection` command: probe an account's servers without
//! persisting anything.
//!
//! The incoming and outgoing servers are probed independently so the setup
//! UI can report exactly which one failed. Nothing is stored: the passwords
//! come straight from the form input and go nowhere but the live connection
//! attempt, and no account row or keychain entry is created.

use std::num::NonZeroU16;

use axomail_core::{IncomingMailProvider, IncomingProtocol, MailError, ServerEndpoint};
use axomail_imap::{ImapConfig, ImapProvider};
use axomail_pop3::{Pop3Config, Pop3Provider};
use axomail_smtp::{SmtpConfig, SmtpProvider};

use crate::dto::{ConnectionTestResult, CreateAccountInput, IncomingInput, ServerInput};

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
