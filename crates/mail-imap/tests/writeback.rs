//! Integration tests for the write-back primitives against the scripted TLS
//! IMAP server: `select_mailbox` as a lightweight selection, and `move_message`
//! over both the `MOVE`-capable path and the RFC 3501 COPY+DELETE+EXPUNGE
//! fallback.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::num::NonZeroU16;
use std::time::Duration;

use axomail_core::{
    ConnectionSecurity, FlagDelta, FlagSet, IncomingMailProvider, MailboxName, RemoteMessageId,
    ServerEndpoint, SystemFlags,
};
use axomail_imap::{ImapConfig, ImapProvider};

use common::{Message, Spec, TestServer, TlsMode};

/// Builds a provider configuration that trusts the test server's cert.
fn config_for(server: &TestServer, security: ConnectionSecurity) -> ImapConfig {
    let endpoint = ServerEndpoint::new(
        server.host.clone(),
        NonZeroU16::new(server.port).expect("nonzero port"),
    )
    .expect("valid endpoint");
    let mut config = ImapConfig::new(endpoint, security, "user", "pass");
    config.extra_root_certs_der = vec![server.cert_der.clone()];
    config.timeout = Duration::from_secs(5);
    config
}

/// An IMAP identity under the scripted server's UIDVALIDITY (42).
fn imap_id(uid: u32) -> RemoteMessageId {
    RemoteMessageId::Imap {
        uid,
        uid_validity: 42,
    }
}

#[tokio::test]
async fn select_mailbox_selects_and_enables_store_flags() {
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = vec![Message::new(1)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls));

    provider.connect().await.expect("connect");
    // The new selection primitive alone (no full sync pass) must let a
    // subsequent per-message op resolve.
    provider
        .select_mailbox(&MailboxName::new("INBOX"))
        .await
        .expect("select");
    provider
        .store_flags(
            &imap_id(1),
            &FlagDelta {
                add: FlagSet::from_system(SystemFlags {
                    seen: true,
                    ..SystemFlags::default()
                }),
                remove: FlagSet::default(),
            },
        )
        .await
        .expect("store flags after select");

    assert!(
        server.saw_command("SELECT"),
        "commands: {:?}",
        server.commands()
    );
    assert!(
        server.saw_command("UID STORE 1 +FLAGS (\\Seen)"),
        "commands: {:?}",
        server.commands()
    );
}

#[tokio::test]
async fn move_uses_uid_move_when_capability_present() {
    // The base spec advertises MOVE.
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = vec![Message::new(1)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls));

    provider.connect().await.expect("connect");
    provider
        .select_mailbox(&MailboxName::new("INBOX"))
        .await
        .expect("select");
    provider
        .move_message(&imap_id(1), &MailboxName::new("Archive"))
        .await
        .expect("move");

    assert!(
        server.saw_command("UID MOVE 1 \"Archive\"") || server.saw_command("UID MOVE 1 Archive"),
        "commands: {:?}",
        server.commands()
    );
    // The fallback commands must NOT be used when MOVE is available.
    assert!(
        !server.saw_command("UID COPY"),
        "commands: {:?}",
        server.commands()
    );
    assert!(
        !server.saw_command("+FLAGS (\\Deleted)"),
        "commands: {:?}",
        server.commands()
    );
}

#[tokio::test]
async fn move_falls_back_to_copy_delete_expunge_without_move() {
    // Drop MOVE from the advertised capabilities, keep UIDPLUS for UID EXPUNGE.
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.capabilities = "IMAP4rev1 UIDPLUS SPECIAL-USE".to_owned();
    spec.messages = vec![Message::new(1)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls));

    provider.connect().await.expect("connect");
    provider
        .select_mailbox(&MailboxName::new("INBOX"))
        .await
        .expect("select");
    provider
        .move_message(&imap_id(1), &MailboxName::new("Archive"))
        .await
        .expect("move via fallback");

    let commands = server.commands();
    assert!(!server.saw_command("UID MOVE"), "commands: {commands:?}");
    assert!(server.saw_command("UID COPY 1"), "commands: {commands:?}");
    assert!(
        server.saw_command("UID STORE 1 +FLAGS (\\Deleted)"),
        "commands: {commands:?}"
    );
    assert!(
        server.saw_command("UID EXPUNGE 1"),
        "commands: {commands:?}"
    );
}

#[tokio::test]
async fn fallback_without_uidplus_does_not_purge_unrelated_deleted() {
    // No MOVE and no UIDPLUS: a bare `EXPUNGE` would remove EVERY \Deleted
    // message in the mailbox, including an unrelated one the user flagged. The
    // fallback must copy + mark \Deleted but never issue that mailbox-wide purge.
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.capabilities = "IMAP4rev1 SPECIAL-USE".to_owned();
    spec.messages = vec![Message::new(1), Message::new(2)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls));

    provider.connect().await.expect("connect");
    provider
        .select_mailbox(&MailboxName::new("INBOX"))
        .await
        .expect("select");
    provider
        .move_message(&imap_id(1), &MailboxName::new("Archive"))
        .await
        .expect("move via fallback");

    let commands = server.commands();
    assert!(server.saw_command("UID COPY 1"), "commands: {commands:?}");
    assert!(
        server.saw_command("UID STORE 1 +FLAGS (\\Deleted)"),
        "commands: {commands:?}"
    );
    // Neither a scoped nor a blanket expunge is issued without UIDPLUS.
    assert!(
        !server.saw_command("EXPUNGE"),
        "no expunge may run without UIDPLUS: {commands:?}"
    );
}

#[tokio::test]
async fn fallback_does_not_recopy_a_message_already_moved() {
    // Retry after a completed move: the source no longer holds the UID, so the
    // fallback must detect that and NOT re-`COPY`, which would duplicate the
    // message in the target.
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.capabilities = "IMAP4rev1 UIDPLUS SPECIAL-USE".to_owned();
    // The source reports only uid 2; uid 1 was already moved away.
    spec.messages = vec![Message::new(2)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server, ConnectionSecurity::Tls));

    provider.connect().await.expect("connect");
    provider
        .select_mailbox(&MailboxName::new("INBOX"))
        .await
        .expect("select");
    provider
        .move_message(&imap_id(1), &MailboxName::new("Archive"))
        .await
        .expect("idempotent move is a silent success");

    let commands = server.commands();
    assert!(
        server.saw_command("UID SEARCH"),
        "the fallback checks the source first: {commands:?}"
    );
    assert!(
        !server.saw_command("UID COPY"),
        "an already-moved message is not re-copied: {commands:?}"
    );
}
