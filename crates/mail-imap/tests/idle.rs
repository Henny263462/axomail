//! Integration tests for IMAP IDLE (RFC 2177) push against the scripted TLS
//! server (see `common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::num::NonZeroU16;
use std::time::Duration;

use axomail_core::{
    ConnectionSecurity, IdleOutcome, IncomingMailProvider, MailboxName, ServerEndpoint,
};
use axomail_imap::{ImapConfig, ImapProvider};

use common::{IdleBehavior, Spec, TestServer, TlsMode};

/// Builds a provider configuration that trusts the test server's cert.
fn config_for(server: &TestServer) -> ImapConfig {
    let endpoint = ServerEndpoint::new(
        server.host.clone(),
        NonZeroU16::new(server.port).expect("nonzero port"),
    )
    .expect("valid endpoint");
    let mut config = ImapConfig::new(endpoint, ConnectionSecurity::Tls, "user", "pass");
    config.extra_root_certs_der = vec![server.cert_der.clone()];
    config.timeout = Duration::from_secs(5);
    config
}

/// A spec that advertises IDLE and scripts the given behavior.
fn idle_spec(behavior: IdleBehavior) -> Spec {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.capabilities = "IMAP4rev1 IDLE MOVE UIDPLUS".to_owned();
    spec.idle_behavior = behavior;
    spec
}

/// Counts how many recorded command lines are `IDLE` (renewal re-issues it).
fn idle_count(server: &TestServer) -> usize {
    server
        .commands()
        .iter()
        .filter(|line| line.to_ascii_uppercase().contains("IDLE"))
        .count()
}

#[tokio::test]
async fn supports_idle_reflects_the_advertised_capability() {
    let server = common::start(idle_spec(IdleBehavior::Quiet)).await;
    let mut provider = ImapProvider::new(config_for(&server));

    // Unknown before connect: no capabilities have been read yet.
    assert!(!provider.supports_idle());
    provider.connect().await.expect("connect");
    assert!(provider.supports_idle(), "IDLE advertised => supported");
    provider.disconnect().await.expect("disconnect");
}

#[tokio::test]
async fn supports_idle_false_when_capability_absent() {
    // `Spec::base` capabilities omit IDLE.
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.idle_behavior = IdleBehavior::Quiet;
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server));

    provider.connect().await.expect("connect");
    assert!(
        !provider.supports_idle(),
        "no IDLE capability => not supported"
    );
    provider.disconnect().await.expect("disconnect");
}

#[tokio::test]
async fn exists_push_returns_notified_and_sends_done() {
    let server = common::start(idle_spec(IdleBehavior::PushExists(5))).await;
    let mut provider = ImapProvider::new(config_for(&server));

    provider.connect().await.expect("connect");
    provider
        .select_mailbox(&MailboxName::new("INBOX"))
        .await
        .expect("select");

    let outcome = provider
        .idle(Duration::from_secs(5), std::future::pending::<()>())
        .await
        .expect("idle");

    assert_eq!(outcome, IdleOutcome::Notified, "EXISTS push wakes the wait");
    assert!(server.saw_command("IDLE"), "IDLE issued");
    assert!(server.saw_command("DONE"), "DONE terminates the wait");
    provider.disconnect().await.expect("disconnect");
}

#[tokio::test]
async fn quiet_wait_times_out_and_renews() {
    let server = common::start(idle_spec(IdleBehavior::Quiet)).await;
    let mut provider = ImapProvider::new(config_for(&server));

    provider.connect().await.expect("connect");
    provider
        .select_mailbox(&MailboxName::new("INBOX"))
        .await
        .expect("select");

    // A short renewal window and no server push: the wait times out — the cue
    // to re-issue IDLE before the server's inactivity limit.
    let first = provider
        .idle(Duration::from_millis(200), std::future::pending::<()>())
        .await
        .expect("first idle");
    assert_eq!(first, IdleOutcome::TimedOut);

    // Renewal: the same session idles again, so the server sees a second IDLE.
    let second = provider
        .idle(Duration::from_millis(200), std::future::pending::<()>())
        .await
        .expect("second idle");
    assert_eq!(second, IdleOutcome::TimedOut);

    assert!(
        idle_count(&server) >= 2,
        "renewal re-issued IDLE (saw {})",
        idle_count(&server)
    );
    provider.disconnect().await.expect("disconnect");
}

#[tokio::test]
async fn interrupt_sends_done_and_returns_interrupted() {
    let server = common::start(idle_spec(IdleBehavior::Quiet)).await;
    let mut provider = ImapProvider::new(config_for(&server));

    provider.connect().await.expect("connect");
    provider
        .select_mailbox(&MailboxName::new("INBOX"))
        .await
        .expect("select");

    // A long renewal window, but the interrupt fires promptly (a shutdown/
    // cancel). DONE must still be sent so the session ends cleanly.
    let outcome = provider
        .idle(
            Duration::from_secs(30),
            tokio::time::sleep(Duration::from_millis(50)),
        )
        .await
        .expect("idle");

    assert_eq!(outcome, IdleOutcome::Interrupted);
    assert!(server.saw_command("DONE"), "DONE sent on cancel");
    // The session is still usable after a clean DONE.
    provider.disconnect().await.expect("disconnect");
}

#[tokio::test]
async fn connection_drop_during_idle_surfaces_an_error() {
    let server = common::start(idle_spec(IdleBehavior::DropOnIdle)).await;
    let mut provider = ImapProvider::new(config_for(&server));

    provider.connect().await.expect("connect");
    provider
        .select_mailbox(&MailboxName::new("INBOX"))
        .await
        .expect("select");

    let result = provider
        .idle(Duration::from_secs(5), std::future::pending::<()>())
        .await;

    assert!(result.is_err(), "a dropped connection is an error");
    // The broken session was not returned, so the provider is disconnected and
    // the supervisor will reconnect.
    assert!(!provider.supports_idle());
}
