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
        full_reconcile: false,
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
        highest_modseq: None,
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
        highest_modseq: None,
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
        highest_modseq: None,
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
        highest_modseq: None,
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
async fn folder_crud_sends_the_expected_commands() {
    let server = common::start(Spec::base(TlsMode::Implicit)).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));
    provider.connect().await.expect("connect");

    provider
        .create_mailbox(&MailboxName::new("Work"))
        .await
        .expect("create");
    provider
        .rename_mailbox(&MailboxName::new("Work"), &MailboxName::new("Projects"))
        .await
        .expect("rename");
    provider
        .set_mailbox_subscribed(&MailboxName::new("Projects"), true)
        .await
        .expect("subscribe");
    provider
        .set_mailbox_subscribed(&MailboxName::new("Projects"), false)
        .await
        .expect("unsubscribe");
    provider
        .delete_mailbox(&MailboxName::new("Projects"))
        .await
        .expect("delete");
    provider.disconnect().await.expect("disconnect");

    let commands = server.commands();
    assert!(server.saw_command("CREATE"), "commands: {commands:?}");
    assert!(server.saw_command("RENAME"), "commands: {commands:?}");
    assert!(server.saw_command("DELETE"), "commands: {commands:?}");
    // "UNSUBSCRIBE" contains "SUBSCRIBE", so match a line that subscribes but
    // does not unsubscribe to prove the subscribe was actually sent.
    assert!(
        commands
            .iter()
            .any(|line| line.contains("SUBSCRIBE") && !line.contains("UNSUBSCRIBE")),
        "commands: {commands:?}"
    );
    assert!(
        commands.iter().any(|line| line.contains("UNSUBSCRIBE")),
        "commands: {commands:?}"
    );
}

#[tokio::test]
async fn folder_create_rejection_maps_to_protocol_error() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.fail_command = Some("CREATE".to_owned());
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));
    provider.connect().await.expect("connect");

    let error = provider
        .create_mailbox(&MailboxName::new("Work"))
        .await
        .expect_err("the server rejects CREATE");
    // A tagged NO must surface as an error — never a silent success that would
    // let the command layer record a phantom local folder.
    assert!(matches!(error, MailError::Protocol { .. }), "got {error:?}");
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

// ---------------------------------------------------------------------------
// CONDSTORE / QRESYNC incremental sync (E1). The fallback matrix:
//   * no CONDSTORE          → newest-window flag scan + present-set diff
//   * CONDSTORE only        → CHANGEDSINCE flag delta + present-set diff
//   * CONDSTORE + QRESYNC   → CHANGEDSINCE delta + VANISHED removals
//   * NOMODSEQ / backwards  → treated as no CONDSTORE (legacy scan)
//   * UIDVALIDITY change    → full picture (unchanged)
// ---------------------------------------------------------------------------

/// A spec advertising the given extra capabilities plus a mailbox
/// `HIGHESTMODSEQ` (or `None` for a `NOMODSEQ` select).
fn condstore_spec(extra_caps: &str, highest_modseq: Option<u64>) -> Spec {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.capabilities = format!("IMAP4rev1 UIDPLUS {extra_caps}");
    spec.highest_modseq = highest_modseq;
    spec
}

/// The prior state at UIDVALIDITY 42 with a stored mod-sequence cursor.
fn condstore_state(highest_uid: u32, highest_modseq: u64) -> MailboxSyncState {
    MailboxSyncState {
        uid_validity: Some(42),
        highest_uid: Some(highest_uid),
        highest_modseq: Some(highest_modseq),
    }
}

#[tokio::test]
async fn condstore_first_sync_records_highest_modseq() {
    let mut spec = condstore_spec("CONDSTORE", Some(20));
    spec.messages = vec![Message::new(1), Message::new(2)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider
        .sync(sync_request(MailboxSyncState::default(), 100))
        .await
        .expect("sync");

    assert_eq!(outcome.uid_validity, Some(42));
    assert_eq!(
        outcome.highest_modseq,
        Some(20),
        "the select's HIGHESTMODSEQ bootstraps the cursor"
    );
    assert!(
        server.saw_command("(CONDSTORE)"),
        "SELECT must add CONDSTORE"
    );
    assert!(
        !server.saw_command("ENABLE QRESYNC"),
        "a CONDSTORE-only server must not be sent ENABLE QRESYNC"
    );
}

#[tokio::test]
async fn condstore_changedsince_applies_only_changed_flags_and_keywords() {
    let mut spec = condstore_spec("CONDSTORE", Some(20));
    let mut m1 = Message::new(1);
    m1.modseq = 5;
    m1.flags = vec!["\\Seen".to_owned()];
    let mut m2 = Message::new(2);
    m2.modseq = 15;
    m2.flags = vec!["\\Flagged".to_owned(), "$Junk".to_owned()];
    let mut m3 = Message::new(3);
    m3.modseq = 12;
    m3.flags = vec!["\\Seen".to_owned()];
    spec.messages = vec![m1, m2, m3];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider
        .sync(sync_request(condstore_state(3, 10), 100))
        .await
        .expect("sync");

    // Only messages whose mod-sequence exceeds the stored 10 (uid 2 and 3)
    // are returned; the unchanged uid 1 never appears (no write amplification).
    let mut changed: Vec<u32> = outcome.flag_updates.iter().map(|(id, _)| uid(id)).collect();
    changed.sort_unstable();
    assert_eq!(changed, vec![2, 3]);
    let m2u = outcome
        .flag_updates
        .iter()
        .find(|(id, _)| uid(id) == 2)
        .unwrap();
    assert!(m2u.1.flagged);
    assert!(
        m2u.1.keywords.iter().any(|k| k.as_str() == "$Junk"),
        "keyword flows through the CHANGEDSINCE delta"
    );

    // CONDSTORE-only: removals still come from the present-set diff.
    let present: Vec<u32> = outcome
        .present_remote_ids
        .as_deref()
        .expect("CONDSTORE-only reports the present set")
        .iter()
        .map(uid)
        .collect();
    assert_eq!(present, vec![1, 2, 3]);
    assert!(outcome.removed.is_empty());
    assert_eq!(outcome.highest_modseq, Some(20));
    assert!(server.saw_command("CHANGEDSINCE 10"));
    assert!(
        !server.saw_command("VANISHED"),
        "CONDSTORE-only must not use the VANISHED modifier"
    );
}

#[tokio::test]
async fn qresync_vanished_earlier_reports_exact_removals() {
    let mut spec = condstore_spec("CONDSTORE QRESYNC", Some(20));
    let mut m2 = Message::new(2);
    m2.modseq = 15;
    m2.flags = vec!["\\Flagged".to_owned()];
    // uid 3 is gone; the server reports it via VANISHED (EARLIER).
    spec.messages = vec![Message::new(1), m2, Message::new(4), Message::new(5)];
    spec.vanished = vec![3];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider
        .sync(sync_request(condstore_state(5, 10), 100))
        .await
        .expect("sync");

    let removed: Vec<u32> = outcome.removed.iter().map(uid).collect();
    assert_eq!(removed, vec![3], "VANISHED (EARLIER) names exactly uid 3");
    assert!(
        outcome.present_remote_ids.is_none(),
        "the QRESYNC fast path skips the present-set diff"
    );
    let changed: Vec<u32> = outcome.flag_updates.iter().map(|(id, _)| uid(id)).collect();
    assert_eq!(changed, vec![2]);
    assert!(server.saw_command("ENABLE QRESYNC"));
    assert!(server.saw_command("CHANGEDSINCE 10 VANISHED"));
}

#[tokio::test]
async fn qresync_ambiguous_vanished_falls_back_to_present_set() {
    let mut spec = condstore_spec("CONDSTORE QRESYNC", Some(20));
    spec.messages = vec![
        Message::new(1),
        Message::new(2),
        Message::new(4),
        Message::new(5),
    ];
    // A VANISHED without (EARLIER) is a real-time notification, not a resync
    // delta: the adapter must not delete on it.
    spec.vanished_override = Some("* VANISHED 3".to_owned());
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider
        .sync(sync_request(condstore_state(5, 10), 100))
        .await
        .expect("sync");

    assert!(
        outcome.removed.is_empty(),
        "an ambiguous VANISHED invents no removals"
    );
    let present: Vec<u32> = outcome
        .present_remote_ids
        .as_deref()
        .expect("fallback reports the authoritative present set")
        .iter()
        .map(uid)
        .collect();
    assert_eq!(present, vec![1, 2, 4, 5]);
    assert!(
        !present.contains(&3),
        "uid 3 is still removed, but safely via the present-set diff"
    );
}

#[tokio::test]
async fn qresync_full_reconcile_uses_present_set_not_vanished() {
    let mut spec = condstore_spec("CONDSTORE QRESYNC", Some(20));
    spec.messages = vec![
        Message::new(1),
        Message::new(2),
        Message::new(4),
        Message::new(5),
    ];
    // Would be reported if VANISHED were requested — it must not be on a
    // reconcile pass.
    spec.vanished = vec![3];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let request = SyncRequest {
        mailbox: MailboxId::new(1),
        remote_name: MailboxName::new("INBOX"),
        state: condstore_state(5, 10),
        batch_size: 100,
        full_reconcile: true,
    };
    let outcome = provider.sync(request).await.expect("sync");

    assert!(outcome.removed.is_empty());
    let present: Vec<u32> = outcome
        .present_remote_ids
        .as_deref()
        .expect("the periodic reconcile pass runs the present-set diff")
        .iter()
        .map(uid)
        .collect();
    assert_eq!(present, vec![1, 2, 4, 5]);
    assert!(server.saw_command("CHANGEDSINCE 10"));
    assert!(
        !server.saw_command("VANISHED"),
        "a reconcile pass omits the VANISHED modifier"
    );
}

#[tokio::test]
async fn nomodseq_select_falls_back_to_legacy_scan() {
    let mut spec = condstore_spec("CONDSTORE", None);
    spec.messages = vec![Message::new(1), Message::new(2)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider
        .sync(sync_request(condstore_state(2, 10), 100))
        .await
        .expect("sync");

    assert_eq!(
        outcome.highest_modseq, None,
        "a NOMODSEQ select clears the cursor"
    );
    assert!(
        outcome.present_remote_ids.is_some(),
        "the legacy path diffs the present set"
    );
    assert!(
        !server.saw_command("CHANGEDSINCE"),
        "without a mod-sequence there is no CHANGEDSINCE delta"
    );
}

#[tokio::test]
async fn modseq_backwards_falls_back_and_reheals() {
    // Server's HIGHESTMODSEQ (5) is below the stored cursor (10): a reset.
    let mut spec = condstore_spec("CONDSTORE", Some(5));
    spec.messages = vec![Message::new(1), Message::new(2)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    let outcome = provider
        .sync(sync_request(condstore_state(2, 10), 100))
        .await
        .expect("sync");

    assert_eq!(
        outcome.highest_modseq,
        Some(5),
        "the server's lower value is re-recorded so a later pass resumes"
    );
    assert!(outcome.present_remote_ids.is_some());
    assert!(
        !server.saw_command("CHANGEDSINCE"),
        "a backwards mod-sequence is not trusted for CHANGEDSINCE"
    );
}

#[tokio::test]
async fn condstore_uidvalidity_change_returns_full_picture() {
    let mut spec = condstore_spec("CONDSTORE", Some(30));
    spec.messages = vec![Message::new(1), Message::new(2)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls, "pass"));

    provider.connect().await.expect("connect");
    // Client believed UIDVALIDITY was 999 and held a cursor; the server says 42.
    let state = MailboxSyncState {
        uid_validity: Some(999),
        highest_uid: Some(1),
        highest_modseq: Some(10),
    };
    let outcome = provider.sync(sync_request(state, 100)).await.expect("sync");

    assert_eq!(outcome.uid_validity, Some(42));
    assert_eq!(outcome.new_envelopes.len(), 2);
    assert!(outcome.removed.is_empty());
    assert!(
        outcome.present_remote_ids.is_none(),
        "the full-picture path wipes local state instead of diffing"
    );
    assert_eq!(outcome.highest_modseq, Some(30));
    assert!(
        !server.saw_command("CHANGEDSINCE"),
        "a full picture is not a mod-sequence delta"
    );
}
