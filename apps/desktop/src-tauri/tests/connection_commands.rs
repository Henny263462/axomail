//! Integration tests for `test_connection`, driven against scripted TLS
//! servers from `axomail-test-support`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{ConnectionSecurity, IncomingProtocol};
use axomail_desktop_lib::commands::connection::{probe, probe_incoming, probe_outgoing};
use axomail_desktop_lib::dto::{CreateAccountInput, IncomingInput, ServerInput};
use axomail_test_support::{ScriptedServer, Step, TestTlsIdentity, imap};

/// Builds the IMAP dialog async-imap walks on connect+disconnect: `LOGIN`
/// carries tag `A0001` (the first command) and `LOGOUT` carries `A0002`.
fn imap_success_steps() -> Vec<Step> {
    let mut steps = imap::login_ok("A0001");
    steps.extend(imap::logout("A0002"));
    steps
}

/// Incoming input pointing at a local IMAP test server.
fn imap_input(port: u16) -> IncomingInput {
    IncomingInput {
        protocol: IncomingProtocol::Imap,
        host: "127.0.0.1".to_owned(),
        port,
        security: ConnectionSecurity::Tls,
        username: "user".to_owned(),
        password: "pass".to_owned(),
    }
}

/// Outgoing input pointing at `127.0.0.1:port` over implicit TLS.
fn smtp_input(port: u16) -> ServerInput {
    ServerInput {
        host: "127.0.0.1".to_owned(),
        port,
        security: ConnectionSecurity::Tls,
        username: "user".to_owned(),
        password: "pass".to_owned(),
    }
}

#[tokio::test]
async fn probe_incoming_succeeds_against_scripted_imap() {
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(imap_success_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let result = probe_incoming(&imap_input(server.port()), &roots).await;

    assert!(
        result.is_ok(),
        "expected a successful probe, got {result:?}"
    );
    assert!(server.connection_count() >= 1);
}

#[tokio::test]
async fn probe_incoming_reports_authentication_failure() {
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(imap::login_fail("A0001"), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let error = probe_incoming(&imap_input(server.port()), &roots)
        .await
        .expect_err("login should be rejected");

    assert!(
        matches!(error, axomail_core::MailError::Authentication),
        "expected Authentication, got {error:?}"
    );
    // The redacted message never leaks credentials.
    assert!(!error.to_string().contains("pass"));
}

#[tokio::test]
async fn probe_incoming_rejects_pop3_until_phase_three() {
    let mut input = imap_input(1);
    input.protocol = IncomingProtocol::Pop3;

    let error = probe_incoming(&input, &[])
        .await
        .expect_err("pop3 is not supported yet");
    assert!(matches!(
        error,
        axomail_core::MailError::UnsupportedCapability { .. }
    ));
}

#[tokio::test]
async fn probe_outgoing_constructs_provider_for_valid_input() {
    // The SMTP adapter connects only at send time, so the feasible probe is
    // provider construction (endpoint + strict TLS connector). It must
    // succeed for a well-formed configuration.
    let result = probe_outgoing(&smtp_input(2525), &[]).await;
    assert!(
        result.is_ok(),
        "construction should succeed, got {result:?}"
    );
}

#[tokio::test]
async fn probe_reports_directions_independently() {
    // Incoming IMAP succeeds; outgoing construction also succeeds — the two
    // results are reported independently.
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(imap_success_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let input = CreateAccountInput {
        display_name: "Work".to_owned(),
        email: "klaus@theferbers.eu".to_owned(),
        incoming: imap_input(server.port()),
        outgoing: smtp_input(2525),
    };
    let result = probe(&input, &roots).await;

    assert!(result.incoming_ok);
    assert!(result.incoming_error.is_none());
    assert!(result.outgoing_ok);
    assert!(result.outgoing_error.is_none());
}
