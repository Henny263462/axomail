//! Integration tests driving [`ImapProvider`] against a scripted TLS IMAP
//! server (see `common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::num::NonZeroU16;
use std::time::Duration;

use axomail_core::{
    ConnectionSecurity, FlagDelta, FlagSet, IncomingMailProvider, Keyword, MailError, MailboxId,
    MailboxName, MailboxRole, MailboxSyncState, RemoteMessageId, ServerEndpoint, SyncRequest,
    SystemFlags,
};
use axomail_imap::{ImapConfig, ImapProvider};

use common::{Disruption, Message, Spec, TestServer, TlsMode};

/// Builds a provider configuration that trusts the test server's cert.
fn config_for(server: &TestServer, security: ConnectionSecurity, password: &str) -> ImapConfig {
    let endpoint = ServerEndpoint::new(
        server.host.clone(),
        NonZeroU16::new(server.port).expect("nonzero port"),
    )
    .expect("valid endpoint");
    let mut config = ImapConfig::new(endpoint, security, "user", password);
    config.extra_root_certs_der = vec![server.cert_der.clone()];
    config.timeout = Duration::from_secs(5);
    config
}

/// Builds an XOAUTH2 provider configuration that trusts the test server's
/// cert.
fn xoauth2_config_for(
    server: &TestServer,
    security: ConnectionSecurity,
    access_token: &str,
) -> ImapConfig {
    let endpoint = ServerEndpoint::new(
        server.host.clone(),
        NonZeroU16::new(server.port).expect("nonzero port"),
    )
    .expect("valid endpoint");
    let mut config = ImapConfig::new_xoauth2(endpoint, security, "user", access_token);
    config.extra_root_certs_der = vec![server.cert_der.clone()];
    config.timeout = Duration::from_secs(5);
    config
}

#[tokio::test]
async fn xoauth2_authenticate_succeeds_and_fetches_capabilities() {
    // The scripted server accepts the token carried in `spec.password`.
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.password = "ya29.the-access-token".to_owned();
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(xoauth2_config_for(
        &server,
        ConnectionSecurity::Tls,
        "ya29.the-access-token",
    ));

    provider.connect().await.expect("xoauth2 connect");
    provider.disconnect().await.expect("disconnect");

    assert!(server.saw_command("AUTHENTICATE XOAUTH2"));
}

#[tokio::test]
async fn xoauth2_wrong_token_maps_to_authentication() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.password = "correct-token".to_owned();
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(xoauth2_config_for(
        &server,
        ConnectionSecurity::Tls,
        "wrong-token",
    ));

    let error = provider.connect().await.expect_err("should reject token");
    assert!(matches!(error, MailError::Authentication));
}

/// A sync request for INBOX with the given previous state and batch size.
fn sync_request(state: MailboxSyncState, batch_size: u32) -> SyncRequest {
    SyncRequest {
        mailbox: MailboxId::new(1),
        remote_name: MailboxName::new("INBOX"),
        state,
        batch_size,
    }
}

/// Returns the UID carried by a remote envelope.
fn uid(id: &RemoteMessageId) -> u32 {
    match id {
        RemoteMessageId::Imap { uid, .. } => *uid,
        RemoteMessageId::Pop3 { .. } => panic!("expected an IMAP id"),
    }
}

#[tokio::test]
async fn implicit_tls_connect_and_login() {
    let server = common::start(Spec::base(TlsMode::Implicit)).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    provider.disconnect().await.expect("disconnect");

    assert!(server.saw_command("LOGIN"));
}

#[tokio::test]
async fn starttls_upgrade_then_login() {
    let server = common::start(Spec::base(TlsMode::StartTls)).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::StartTls, "pass"));

    provider.connect().await.expect("connect over starttls");
    provider.disconnect().await.expect("disconnect");

    assert!(server.saw_command("STARTTLS"));
    assert!(server.saw_command("LOGIN"));
}

#[tokio::test]
async fn wrong_password_maps_to_authentication() {
    let server = common::start(Spec::base(TlsMode::Implicit)).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "nope"));

    let error = provider.connect().await.expect_err("should reject login");
    assert!(matches!(error, MailError::Authentication));
}

#[tokio::test]
async fn discover_maps_special_use_and_decodes_utf7() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.list_lines = vec![
        "* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n".to_owned(),
        "* LIST (\\Sent \\HasNoChildren) \"/\" \"Sent\"\r\n".to_owned(),
        "* LIST (\\HasNoChildren) \"/\" \"Gesch&AOQ-fte\"\r\n".to_owned(),
        "* LIST (\\Noselect \\HasChildren) \"/\" \"Stuff\"\r\n".to_owned(),
    ];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let state = provider.discover().await.expect("discover");

    let names: Vec<&str> = state.mailboxes.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"INBOX"));
    assert!(names.contains(&"Sent"));
    assert!(names.contains(&"Geschäfte"), "utf7 name should decode");
    assert!(!names.contains(&"Stuff"), "\\Noselect must be filtered out");

    let sent = state
        .mailboxes
        .iter()
        .find(|m| m.name.as_str() == "Sent")
        .unwrap();
    assert_eq!(sent.role, Some(MailboxRole::Sent));
    let inbox = state
        .mailboxes
        .iter()
        .find(|m| m.name.as_str() == "INBOX")
        .unwrap();
    assert_eq!(inbox.role, None, "INBOX carries no SPECIAL-USE hint");
}

#[tokio::test]
async fn initial_sync_returns_full_picture_with_decoded_headers() {
    let mut spec = Spec::base(TlsMode::Implicit);
    let mut first = Message::new(1);
    first.subject = "=?UTF-8?Q?Caf=C3=A9?=".to_owned();
    first.from_name = "=?UTF-8?B?QW5kcsOp?=".to_owned();
    first.from_mailbox = "andre".to_owned();
    spec.messages = vec![first, Message::new(2), Message::new(3)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider
        .sync(sync_request(MailboxSyncState::default(), 100))
        .await
        .expect("sync");

    assert_eq!(outcome.uid_validity, Some(42));
    assert_eq!(outcome.highest_uid, Some(3));
    assert!(!outcome.has_more);
    assert_eq!(outcome.new_envelopes.len(), 3);
    assert!(outcome.removed.is_empty());
    assert!(outcome.flag_updates.is_empty());

    let env = outcome
        .new_envelopes
        .iter()
        .find(|e| uid(&e.remote_id) == 1)
        .unwrap();
    assert_eq!(env.subject.as_deref(), Some("Café"));
    assert_eq!(env.from.len(), 1);
    assert_eq!(env.from[0].display_name.as_deref(), Some("André"));
    assert_eq!(env.from[0].email.as_str(), "andre@example.org");
    assert!(env.date.is_some());
    assert!(env.size_bytes.is_some());
}

#[tokio::test]
async fn initial_sync_respects_batch_size() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = vec![Message::new(1), Message::new(2), Message::new(3)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider
        .sync(sync_request(MailboxSyncState::default(), 2))
        .await
        .expect("sync");

    assert_eq!(outcome.new_envelopes.len(), 2);
    assert!(outcome.has_more);
    assert_eq!(outcome.highest_uid, Some(2));
}

#[tokio::test]
async fn incremental_sync_fetches_new_uids() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = vec![
        Message::new(1),
        Message::new(2),
        Message::new(3),
        Message::new(4),
    ];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let state = MailboxSyncState {
        uid_validity: Some(42),
        highest_uid: Some(2),
    };
    let outcome = provider.sync(sync_request(state, 100)).await.expect("sync");

    let mut new_uids: Vec<u32> = outcome
        .new_envelopes
        .iter()
        .map(|e| uid(&e.remote_id))
        .collect();
    new_uids.sort_unstable();
    assert_eq!(new_uids, vec![3, 4]);
    assert_eq!(outcome.highest_uid, Some(4));
    assert!(!outcome.has_more);
    assert!(outcome.removed.is_empty());

    // Regression (data-loss bug): the present set must contain the UIDs
    // fetched in this very pass — the ingest layer deletes every stored
    // row absent from it, in the same transaction that inserts them.
    let present: Vec<u32> = outcome
        .present_remote_ids
        .as_deref()
        .expect("incremental sync reports the present set")
        .iter()
        .map(uid)
        .collect();
    assert_eq!(present, vec![1, 2, 3, 4]);
}

#[tokio::test]
async fn uidvalidity_change_returns_full_picture() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = vec![Message::new(1), Message::new(2)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    // The client believed UIDVALIDITY was 999; the server reports 42.
    let state = MailboxSyncState {
        uid_validity: Some(999),
        highest_uid: Some(1),
    };
    let outcome = provider.sync(sync_request(state, 100)).await.expect("sync");

    assert_eq!(outcome.uid_validity, Some(42));
    assert_eq!(outcome.new_envelopes.len(), 2);
    assert!(
        outcome.removed.is_empty(),
        "full picture reports no removals"
    );
    assert_eq!(outcome.highest_uid, Some(2));
}

#[tokio::test]
async fn incremental_sync_detects_removed_messages() {
    let mut spec = Spec::base(TlsMode::Implicit);
    // UID 3 is gone; 1, 2, 4, 5 remain.
    spec.messages = vec![
        Message::new(1),
        Message::new(2),
        Message::new(4),
        Message::new(5),
    ];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let state = MailboxSyncState {
        uid_validity: Some(42),
        highest_uid: Some(5),
    };
    let outcome = provider.sync(sync_request(state, 100)).await.expect("sync");

    // The adapter reports the authoritative present set (bounded by real
    // messages) instead of enumerating absent UIDs; the caller diffs its
    // stored ids against it. UID 3 must be absent from the present set.
    assert!(outcome.removed.is_empty());
    let present: Vec<u32> = outcome
        .present_remote_ids
        .as_deref()
        .expect("incremental sync reports the present set")
        .iter()
        .map(uid)
        .collect();
    assert_eq!(present, vec![1, 2, 4, 5]);
    assert!(!present.contains(&3));
    assert!(outcome.new_envelopes.is_empty());
}

#[tokio::test]
async fn incremental_sync_reports_flag_updates() {
    let mut spec = Spec::base(TlsMode::Implicit);
    let mut m1 = Message::new(1);
    m1.flags = vec!["\\Seen".to_owned()];
    let mut m2 = Message::new(2);
    m2.flags = vec!["\\Flagged".to_owned()];
    spec.messages = vec![m1, m2];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let state = MailboxSyncState {
        uid_validity: Some(42),
        highest_uid: Some(2),
    };
    let outcome = provider.sync(sync_request(state, 100)).await.expect("sync");

    assert_eq!(outcome.flag_updates.len(), 2);
    let seen = outcome
        .flag_updates
        .iter()
        .find(|(id, _)| uid(id) == 1)
        .unwrap();
    assert!(seen.1.seen);
    let flagged = outcome
        .flag_updates
        .iter()
        .find(|(id, _)| uid(id) == 2)
        .unwrap();
    assert!(flagged.1.flagged);
}

#[tokio::test]
async fn sync_reads_custom_flags_as_keywords() {
    let mut spec = Spec::base(TlsMode::Implicit);
    let mut message = Message::new(1);
    // A system flag plus two custom keywords and a non-keyword server token.
    message.flags = vec![
        "\\Seen".to_owned(),
        "$Junk".to_owned(),
        "$label2".to_owned(),
        "\\Recent".to_owned(),
    ];
    spec.messages = vec![message];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider
        .sync(sync_request(MailboxSyncState::default(), 100))
        .await
        .expect("sync");

    let envelope = &outcome.new_envelopes[0];
    assert!(envelope.flags.seen);
    let keywords: Vec<&str> = envelope
        .flags
        .keywords
        .iter()
        .map(Keyword::as_str)
        .collect();
    // Both custom flags become keywords; the session-only `\Recent` does not.
    assert!(keywords.contains(&"$Junk"));
    assert!(keywords.contains(&"$label2"));
    assert_eq!(keywords.len(), 2, "keywords: {keywords:?}");
}

#[tokio::test]
async fn fetch_message_returns_full_bytes_via_peek() {
    let mut spec = Spec::base(TlsMode::Implicit);
    let mut message = Message::new(1);
    message.body = "Subject: Hi\r\n\r\nHello world\r\n".to_owned();
    spec.messages = vec![message.clone()];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    provider
        .sync(sync_request(MailboxSyncState::default(), 100))
        .await
        .expect("sync selects the mailbox");

    let raw = provider
        .fetch_message(&RemoteMessageId::Imap {
            uid: 1,
            uid_validity: 42,
        })
        .await
        .expect("fetch message");

    assert_eq!(raw.bytes, message.body.into_bytes());
    assert!(
        server.saw_command("BODY.PEEK[]"),
        "fetch must use PEEK to avoid setting \\Seen"
    );
}

#[tokio::test]
async fn store_flags_sends_additive_store() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = vec![Message::new(1)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    provider
        .sync(sync_request(MailboxSyncState::default(), 100))
        .await
        .expect("sync selects the mailbox");

    // Add \Seen \Flagged and the $Junk keyword; remove \Deleted. The write is
    // additive per side, never a replace, so untouched flags survive.
    let delta = FlagDelta {
        add: FlagSet {
            system: SystemFlags {
                seen: true,
                flagged: true,
                ..SystemFlags::default()
            },
            keywords: [Keyword::parse("$Junk").expect("valid keyword")]
                .into_iter()
                .collect(),
        },
        remove: FlagSet::from_system(SystemFlags {
            deleted: true,
            ..SystemFlags::default()
        }),
    };
    provider
        .store_flags(
            &RemoteMessageId::Imap {
                uid: 1,
                uid_validity: 42,
            },
            &delta,
        )
        .await
        .expect("store flags");

    assert!(
        server.saw_command("UID STORE 1 +FLAGS (\\Seen \\Flagged $Junk)"),
        "commands: {:?}",
        server.commands()
    );
    assert!(
        server.saw_command("UID STORE 1 -FLAGS (\\Deleted)"),
        "commands: {:?}",
        server.commands()
    );
    // A replace-form STORE must never be emitted.
    assert!(
        !server.saw_command("UID STORE 1 FLAGS ("),
        "commands: {:?}",
        server.commands()
    );
}

#[tokio::test]
async fn stalled_server_times_out() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.disruption = Disruption::StallAfterLogin;
    let server = common::start(spec).await;
    let mut config = config_for(&server, ConnectionSecurity::Tls, "pass");
    config.timeout = Duration::from_millis(300);
    let mut provider = ImapProvider::new(config);

    provider
        .connect()
        .await
        .expect("connect succeeds before stall");
    let error = provider
        .discover()
        .await
        .expect_err("discover should time out");
    assert!(matches!(error, MailError::Timeout { .. }), "got {error:?}");
}

#[tokio::test]
async fn connection_closed_mid_command_is_network_or_protocol() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.disruption = Disruption::CloseAfterLogin;
    spec.messages = vec![Message::new(1)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider
        .connect()
        .await
        .expect("connect succeeds before close");
    let error = provider
        .sync(sync_request(MailboxSyncState::default(), 100))
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
