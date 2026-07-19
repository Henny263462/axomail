//! Integration tests for `append_message` against the scripted TLS IMAP
//! server: a Sent copy is uploaded with `\Seen`, a draft with `\Draft`, the
//! appended UID is adopted by matching `Message-ID`, and an `APPEND` the server
//! rejects surfaces as an error (so the caller can keep the send successful).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::num::NonZeroU16;
use std::time::Duration;

use axomail_core::{
    ConnectionSecurity, FlagSet, IncomingMailProvider, MailboxName, RemoteMessageId,
    ServerEndpoint, SystemFlags,
};
use axomail_imap::{ImapConfig, ImapProvider};

use common::{Message, Spec, TestServer, TlsMode};

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

/// A `\Seen` (Sent-copy) flag set.
fn seen() -> FlagSet {
    FlagSet::from_system(SystemFlags {
        seen: true,
        ..SystemFlags::default()
    })
}

/// A `\Draft` flag set.
fn draft() -> FlagSet {
    FlagSet::from_system(SystemFlags {
        draft: true,
        ..SystemFlags::default()
    })
}

/// A minimal raw message carrying a `Message-ID`.
const RAW: &[u8] = b"Message-ID: <sent.1@example.org>\r\nSubject: Hi\r\n\r\nBody\r\n";

#[tokio::test]
async fn appends_sent_copy_with_seen_flag() {
    let server = common::start(Spec::base(TlsMode::Implicit)).await;
    let mut provider = ImapProvider::new(config_for(&server));

    provider.connect().await.expect("connect");
    provider
        .append_message(&MailboxName::new("Sent"), &seen(), RAW)
        .await
        .expect("append sent copy");

    let commands = server.commands();
    assert!(
        commands.iter().any(|line| line.contains("APPEND")
            && line.contains("Sent")
            && line.contains("\\Seen")),
        "commands: {commands:?}"
    );
}

#[tokio::test]
async fn appends_draft_with_draft_flag() {
    let server = common::start(Spec::base(TlsMode::Implicit)).await;
    let mut provider = ImapProvider::new(config_for(&server));

    provider.connect().await.expect("connect");
    provider
        .append_message(&MailboxName::new("Drafts"), &draft(), RAW)
        .await
        .expect("append draft");

    let commands = server.commands();
    assert!(
        commands.iter().any(|line| {
            line.contains("APPEND") && line.contains("Drafts") && line.contains("\\Draft")
        }),
        "commands: {commands:?}"
    );
}

#[tokio::test]
async fn adopts_the_appended_uid_by_message_id() {
    // The target folder reports one message; adoption selects it and searches
    // by Message-ID, resolving the appended copy's server identity.
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.messages = vec![Message::new(1001)];
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server));

    provider.connect().await.expect("connect");
    let outcome = provider
        .append_message(&MailboxName::new("Sent"), &seen(), RAW)
        .await
        .expect("append sent copy");

    assert_eq!(
        outcome.remote_id,
        Some(RemoteMessageId::Imap {
            uid: 1001,
            uid_validity: 42,
        }),
        "the appended UID should be adopted; commands: {:?}",
        server.commands()
    );
    // Adoption matches by Message-ID header.
    assert!(
        server
            .commands()
            .iter()
            .any(|line| line.contains("UID SEARCH") && line.contains("MESSAGE-ID")),
        "commands: {:?}",
        server.commands()
    );
}

#[tokio::test]
async fn append_failure_surfaces_as_an_error() {
    // A server that rejects APPEND lets the caller keep the send successful and
    // record the upload failure separately.
    let mut spec = Spec::base(TlsMode::Implicit);
    spec.fail_command = Some("APPEND".to_owned());
    let server = common::start(spec).await;
    let mut provider = ImapProvider::new(config_for(&server));

    provider.connect().await.expect("connect");
    let error = provider
        .append_message(&MailboxName::new("Sent"), &seen(), RAW)
        .await
        .expect_err("a rejected APPEND must error");
    // A protocol-level rejection, not a silent success.
    assert!(
        matches!(error, axomail_core::MailError::Protocol { .. }),
        "unexpected error: {error:?}"
    );
}
