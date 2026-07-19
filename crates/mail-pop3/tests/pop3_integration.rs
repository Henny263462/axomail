//! Integration tests driving [`Pop3Provider`] against a scripted TLS POP3
//! server (see `common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashSet;
use std::time::Duration;

use axomail_core::{
    ConnectionCredential, ConnectionSecurity, IncomingMailProvider, MailError, MailboxId,
    MailboxName, MailboxRole, MailboxSyncState, RemoteMessageId, SyncRequest,
};
use axomail_pop3::Pop3Provider;

use common::{Disruption, Spec, TestMessage, TlsMode};

/// A sync request for INBOX with a default state and the given batch size.
fn sync_request(batch_size: u32) -> SyncRequest {
    SyncRequest {
        mailbox: MailboxId::new(1),
        remote_name: MailboxName::new("INBOX"),
        state: MailboxSyncState::default(),
        batch_size,
    }
}

/// Returns the `UIDL` carried by a POP3 remote id.
fn uidl(id: &RemoteMessageId) -> &str {
    match id {
        RemoteMessageId::Pop3 { uidl } => uidl,
        RemoteMessageId::Imap { .. } => panic!("expected a POP3 id"),
    }
}

/// Two baseline messages with explicit `UIDL`s.
fn two_messages() -> Vec<TestMessage> {
    vec![
        TestMessage::new("UID-0001", "Weekly report", "Alice <alice@example.org>"),
        TestMessage::new("UID-0002", "Lunch?", "bob@example.org"),
    ]
}

#[tokio::test]
async fn implicit_tls_connect_and_auth() {
    let server = common::start(Spec::base(TlsMode::Implicit)).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    provider.disconnect().await.expect("disconnect");

    assert!(server.saw_command("USER"));
    assert!(server.saw_command("PASS"));
    assert!(server.saw_command("QUIT"));
}

#[tokio::test]
async fn starttls_upgrade_then_sync() {
    let mut spec = Spec::base(TlsMode::StartTls);
    spec.messages = two_messages();
    let server = common::start(spec).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::StartTls, "pass"));

    provider.connect().await.expect("connect over starttls");
    let outcome = provider.sync(sync_request(100)).await.expect("sync");
    provider.disconnect().await.expect("disconnect");

    assert!(server.saw_command("STLS"));
    assert!(server.saw_command("PASS"));
    assert_eq!(
        outcome.new_envelopes.len(),
        2,
        "sync ran over the TLS channel"
    );
}

#[tokio::test]
async fn starttls_authenticates_with_sasl_advertised_only_after_tls() {
    // The server hides `SASL XOAUTH2` from the plaintext CAPA and only reveals
    // it in the CAPA issued over TLS. The adapter must therefore re-query
    // capabilities after the handshake, or it would reject XOAUTH2 up front.
    let mut spec = Spec::base(TlsMode::StartTls);
    spec.advertise_sasl = true;
    spec.messages = two_messages();
    let server = common::start(spec).await;

    let mut config = server.config(ConnectionSecurity::StartTls, "pass");
    config.credential = ConnectionCredential::xoauth2("access-token-xyz");
    let mut provider = Pop3Provider::new(config);

    provider
        .connect()
        .await
        .expect("XOAUTH2 auth over TLS after re-issued CAPA");
    provider.disconnect().await.expect("disconnect");

    assert!(server.saw_command("STLS"));
    assert!(
        server.saw_command("AUTH XOAUTH2"),
        "the adapter must attempt XOAUTH2 once TLS-phase CAPA advertises SASL"
    );
}

#[tokio::test]
async fn starttls_unsupported_maps_to_unsupported_capability() {
    let mut spec = Spec::base(TlsMode::StartTls);
    spec.advertise_stls = false;
    let server = common::start(spec).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::StartTls, "pass"));

    let error = provider
        .connect()
        .await
        .expect_err("STLS should be refused");
    assert!(
        matches!(error, MailError::UnsupportedCapability { .. }),
        "got {error:?}"
    );
    assert!(
        !server.saw_command("STLS"),
        "must not send STLS when unadvertised"
    );
}

#[tokio::test]
async fn wrong_password_maps_to_authentication() {
    let server = common::start(Spec::base(TlsMode::Implicit)).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::Tls, "nope"));

    let error = provider.connect().await.expect_err("should reject login");
    assert!(matches!(error, MailError::Authentication), "got {error:?}");
}

#[tokio::test]
async fn discover_returns_single_inbox() {
    let server = common::start(Spec::base(TlsMode::Implicit)).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let state = provider.discover().await.expect("discover");

    assert_eq!(state.mailboxes.len(), 1);
    assert_eq!(state.mailboxes[0].name.as_str(), "INBOX");
    assert_eq!(state.mailboxes[0].role, Some(MailboxRole::Inbox));
}

#[tokio::test]
async fn uidl_sync_reports_new_messages() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = two_messages();
    let server = common::start(spec).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider.sync(sync_request(100)).await.expect("sync");

    assert_eq!(outcome.uid_validity, None, "POP3 has no UIDVALIDITY");
    assert_eq!(outcome.highest_uid, None);
    assert!(!outcome.has_more);
    let mut uidls: Vec<&str> = outcome
        .new_envelopes
        .iter()
        .map(|e| uidl(&e.remote_id))
        .collect();
    uidls.sort_unstable();
    assert_eq!(uidls, vec!["UID-0001", "UID-0002"]);
    assert!(outcome.removed.is_empty());
}

#[tokio::test]
async fn uidl_sync_skips_known_and_reports_removed() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = two_messages();
    let server = common::start(spec).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::Tls, "pass"));

    // The engine already has UID-0001 stored, and a UID-9999 that has since
    // disappeared from the server.
    provider.set_known_uidls(HashSet::from([
        "UID-0001".to_owned(),
        "UID-9999".to_owned(),
    ]));

    provider.connect().await.expect("connect");
    let outcome = provider.sync(sync_request(100)).await.expect("sync");

    assert_eq!(outcome.new_envelopes.len(), 1, "only UID-0002 is new");
    assert_eq!(uidl(&outcome.new_envelopes[0].remote_id), "UID-0002");
    assert!(
        !server.saw_command("TOP 1"),
        "known message must not be TOPped"
    );
    assert!(server.saw_command("TOP 2"), "new message envelope uses TOP");

    let removed: Vec<&str> = outcome.removed.iter().map(uidl).collect();
    assert_eq!(removed, vec!["UID-9999"]);
}

#[tokio::test]
async fn top_based_envelope_fields_are_populated() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = vec![TestMessage::new(
        "UID-0001",
        "Weekly report",
        "Alice <alice@example.org>",
    )];
    let server = common::start(spec).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider.sync(sync_request(100)).await.expect("sync");

    let env = &outcome.new_envelopes[0];
    assert_eq!(env.subject.as_deref(), Some("Weekly report"));
    assert_eq!(env.from.len(), 1);
    assert_eq!(env.from[0].display_name.as_deref(), Some("Alice"));
    assert_eq!(env.from[0].email.as_str(), "alice@example.org");
    assert!(env.date.is_some());
    assert!(env.size_bytes.is_some());
}

#[tokio::test]
async fn no_top_leaves_envelope_fields_empty_but_keeps_size() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.advertise_top = false;
    spec.top_ok = false;
    spec.messages = two_messages();
    let server = common::start(spec).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider.sync(sync_request(100)).await.expect("sync");

    assert!(
        !server.saw_command("TOP"),
        "TOP must be skipped when unadvertised"
    );
    for env in &outcome.new_envelopes {
        assert!(env.subject.is_none());
        assert!(env.from.is_empty());
        assert!(env.date.is_none());
        assert!(env.size_bytes.is_some(), "size still comes from LIST");
    }
}

#[tokio::test]
async fn missing_uidl_falls_back_to_digest_identity() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.advertise_uidl = false;
    spec.uidl_ok = false;
    spec.messages = two_messages();
    let server = common::start(spec).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider.sync(sync_request(100)).await.expect("sync");

    assert!(server.saw_command("UIDL"), "the adapter probes UIDL first");
    assert_eq!(outcome.new_envelopes.len(), 2);
    for env in &outcome.new_envelopes {
        assert!(
            uidl(&env.remote_id).starts_with("d:"),
            "digest identity expected, got {}",
            uidl(&env.remote_id)
        );
        // Headers were fetched for the digest and reused for the envelope.
        assert!(env.subject.is_some());
    }
}

#[tokio::test]
async fn digest_identity_is_stable_across_syncs() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.advertise_uidl = false;
    spec.uidl_ok = false;
    spec.messages = two_messages();
    let server = common::start(spec).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let first = provider.sync(sync_request(100)).await.expect("first sync");
    let learned: HashSet<String> = first
        .new_envelopes
        .iter()
        .map(|e| uidl(&e.remote_id).to_owned())
        .collect();

    // Feed the learned digests back and re-sync: nothing should look new.
    provider.set_known_uidls(learned);
    let second = provider.sync(sync_request(100)).await.expect("second sync");
    assert!(
        second.new_envelopes.is_empty(),
        "stable digests must dedup on the next pass"
    );
    assert!(second.removed.is_empty());
}

#[tokio::test]
async fn fetch_message_round_trips_dot_stuffed_body() {
    let dotted = TestMessage {
        uidl: "UID-DOT".to_owned(),
        full: concat!(
            "From: alice@example.org\r\n",
            "Subject: Dotted body\r\n",
            "\r\n",
            "Line one\r\n",
            ".hidden leading dot\r\n",
            "..double dot\r\n",
            "Last line\r\n",
        )
        .to_owned(),
    };
    let expected = dotted.full.clone().into_bytes();

    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = vec![dotted];
    let server = common::start(spec).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    provider
        .sync(sync_request(100))
        .await
        .expect("sync builds the map");
    let raw = provider
        .fetch_message(&RemoteMessageId::Pop3 {
            uidl: "UID-DOT".to_owned(),
        })
        .await
        .expect("fetch");

    assert_eq!(raw.bytes, expected, "dot-stuffing must round-trip exactly");
}

#[tokio::test]
async fn delete_is_blocked_by_default_policy() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = two_messages();
    let server = common::start(spec).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    provider.sync(sync_request(100)).await.expect("sync");
    let error = provider
        .delete_message(&RemoteMessageId::Pop3 {
            uidl: "UID-0001".to_owned(),
        })
        .await
        .expect_err("delete must be refused by default");
    provider.disconnect().await.expect("disconnect");

    assert!(
        matches!(error, MailError::UnsupportedCapability { .. }),
        "got {error:?}"
    );
    assert!(!server.saw_command("DELE"), "no DELE may reach the server");
}

#[tokio::test]
async fn delete_with_policy_sends_dele_then_commits_on_quit() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = two_messages();
    let server = common::start(spec).await;
    let mut config = server.config(ConnectionSecurity::Tls, "pass");
    config.delete_after_download = true;
    let mut provider = Pop3Provider::new(config);

    provider.connect().await.expect("connect");
    provider.sync(sync_request(100)).await.expect("sync");
    provider
        .delete_message(&RemoteMessageId::Pop3 {
            uidl: "UID-0002".to_owned(),
        })
        .await
        .expect("delete allowed under policy");
    provider
        .disconnect()
        .await
        .expect("disconnect commits DELE");

    assert!(server.saw_command("DELE 2"), "DELE for message 2 expected");
    assert!(server.saw_command("QUIT"), "QUIT commits the deletion");
    // DELE must precede QUIT in the recorded order.
    let commands = server.commands();
    let dele = commands.iter().position(|c| c.starts_with("DELE")).unwrap();
    let quit = commands.iter().position(|c| c.starts_with("QUIT")).unwrap();
    assert!(dele < quit, "DELE must be sent before QUIT");
}

#[tokio::test]
async fn stalled_server_times_out() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.disruption = Disruption::StallAfterAuth;
    let server = common::start(spec).await;
    let mut config = server.config(ConnectionSecurity::Tls, "pass");
    config.timeout = Duration::from_millis(300);
    let mut provider = Pop3Provider::new(config);

    provider.connect().await.expect("connect before stall");
    let error = provider
        .sync(sync_request(100))
        .await
        .expect_err("sync should time out");
    assert!(matches!(error, MailError::Timeout { .. }), "got {error:?}");
}

#[tokio::test]
async fn connection_closed_mid_command_is_network_or_protocol() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.disruption = Disruption::CloseAfterAuth;
    spec.messages = two_messages();
    let server = common::start(spec).await;
    let mut provider = Pop3Provider::new(server.config(ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect before close");
    let error = provider
        .sync(sync_request(100))
        .await
        .expect_err("sync should fail after the server drops the connection");
    assert!(
        matches!(
            error,
            MailError::Network { .. } | MailError::Protocol { .. }
        ),
        "got {error:?}"
    );
}
