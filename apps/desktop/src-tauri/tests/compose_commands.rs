//! Integration tests for the send/outbox commands, driven against a scripted
//! TLS SMTP server from `axomail-test-support`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{AccountId, MailboxId, MailboxRole};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::compose::{
    list_outbox_impl, retry_outbox_impl, send_message_impl,
};
use axomail_storage::{NewOutbox, OutboxId, OutboxState, Store};
use axomail_test_support::{ScriptedServer, Step, TestTlsIdentity};
use common::{compose_input, outgoing_account_input, secret_stores};

/// Builds a scripted SMTP submission dialog. Advertising only `AUTH PLAIN`
/// forces mail-send to authenticate in a single inline command.
fn smtp_steps(auth_ok: bool, rcpt_reply: &str) -> Vec<Step> {
    let mut steps = vec![
        Step::send_line("220 axomail.test ESMTP ready"),
        Step::expect_prefix("EHLO"),
        Step::send_line("250-axomail.test greets you"),
        Step::send_line("250-AUTH PLAIN"),
        Step::send_line("250 SIZE 52428800"),
        Step::expect_prefix("AUTH"),
    ];
    if !auth_ok {
        steps.push(Step::send_line("535 5.7.8 authentication failed"));
        return steps;
    }
    steps.push(Step::send_line("235 2.7.0 authentication succeeded"));
    steps.push(Step::expect_prefix("MAIL FROM"));
    steps.push(Step::send_line("250 2.1.0 sender ok"));
    steps.push(Step::expect_prefix("RCPT TO"));
    steps.push(Step::send_line(rcpt_reply.to_owned()));
    if rcpt_reply.starts_with("250") {
        steps.push(Step::expect_prefix("DATA"));
        steps.push(Step::send_line("354 end data with <CR><LF>.<CR><LF>"));
        steps.push(Step::expect_until("."));
        steps.push(Step::send_line("250 2.0.0 accepted for delivery"));
        steps.push(Step::expect_prefix("QUIT"));
        steps.push(Step::send_line("221 2.0.0 bye"));
        steps.push(Step::Close);
    }
    steps
}

/// Finds the id of the Sent mailbox in an account summary.
fn sent_mailbox_id(mailboxes: &[axomail_desktop_lib::dto::MailboxSummaryDto]) -> MailboxId {
    let id = mailboxes
        .iter()
        .find(|mailbox| mailbox.role == MailboxRole::Sent)
        .expect("account has a Sent mailbox")
        .id;
    MailboxId::new(id)
}

#[tokio::test]
async fn send_message_happy_path_files_sent_copy_and_captures_data() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_steps(true, "250 2.1.5 ok"), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let summary = create_account_impl(&store, &secrets, outgoing_account_input(server.port()))
        .await
        .unwrap();

    let input = compose_input(summary.id, &["bob@example.org"], "Meeting notes");
    let outcome = send_message_impl(&store, &secrets, &roots, input)
        .await
        .unwrap();

    assert!(outcome.sent, "send should succeed");
    assert!(outcome.error.is_none());

    // The outbox row reached `sent` after one attempt.
    let items = list_outbox_impl(&store).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].state, "sent");
    assert_eq!(items[0].attempts, 1);

    // A local copy landed in the Sent mailbox.
    let sent = sent_mailbox_id(&summary.mailboxes);
    let page = store.messages().list_page(sent, None, 10).await.unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].subject.as_deref(), Some("Meeting notes"));

    // The server received the DATA payload.
    let transcript = server.transcripts().into_iter().next().unwrap();
    assert!(transcript.received_text().contains("Meeting notes"));
}

#[tokio::test]
async fn send_message_permanent_failure_marks_failed_without_duplicate() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    // A 550 that echoes an address, to also exercise redaction.
    let server = ScriptedServer::spawn_tls(
        smtp_steps(true, "550 5.1.1 <bob@example.org> unknown user"),
        &identity,
    )
    .await
    .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let summary = create_account_impl(&store, &secrets, outgoing_account_input(server.port()))
        .await
        .unwrap();

    let input = compose_input(summary.id, &["bob@example.org"], "Undeliverable");
    let outcome = send_message_impl(&store, &secrets, &roots, input)
        .await
        .unwrap();

    assert!(!outcome.sent);
    let error = outcome.error.expect("failure carries a reason");
    assert!(
        !error.contains("bob@example.org"),
        "echoed address must be redacted: {error}"
    );

    let items = list_outbox_impl(&store).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].state, "failed");
    assert_eq!(items[0].attempts, 1);
    assert!(items[0].last_error.is_some());

    // No local copy was filed for a message that never left.
    let sent = sent_mailbox_id(&summary.mailboxes);
    let page = store.messages().list_page(sent, None, 10).await.unwrap();
    assert!(page.items.is_empty());

    // The guarded transition prevents a duplicate submission: once a row is
    // `sending`, `begin_send` refuses to move it again.
    let id = OutboxId::new(items[0].id);
    assert!(
        store.outbox().begin_send(id).await.unwrap(),
        "failed -> sending"
    );
    assert!(
        !store.outbox().begin_send(id).await.unwrap(),
        "sending -> rejected (no duplicate)"
    );
}

#[tokio::test]
async fn retry_outbox_sends_queued_row_then_guards_a_sent_row() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_steps(true, "250 2.1.5 ok"), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let summary = create_account_impl(&store, &secrets, outgoing_account_input(server.port()))
        .await
        .unwrap();

    // A row queued without going through send_message.
    let record = store
        .outbox()
        .create(&NewOutbox {
            account: AccountId::new(summary.id),
            draft_id: None,
            identity_id: None,
            subject: "Queued draft".to_owned(),
            body_text: "body".to_owned(),
            to: vec!["bob@example.org".to_owned()],
            cc: Vec::new(),
            bcc: Vec::new(),
            next_attempt_unix: None,
        })
        .await
        .unwrap();

    let outcome = retry_outbox_impl(&store, &secrets, &roots, record.id.value())
        .await
        .unwrap();
    assert!(outcome.sent);

    let got = store.outbox().get(record.id).await.unwrap().unwrap();
    assert_eq!(got.state, OutboxState::Sent);
    assert_eq!(got.attempts, 1);

    // Retrying a sent row reports success but does not resubmit.
    let again = retry_outbox_impl(&store, &secrets, &roots, record.id.value())
        .await
        .unwrap();
    assert!(again.sent);
    let after = store.outbox().get(record.id).await.unwrap().unwrap();
    assert_eq!(after.attempts, 1, "a sent row must not be resubmitted");
    assert_eq!(server.connection_count(), 1, "only one submission happened");
}

#[tokio::test]
async fn retry_outbox_rejects_an_unknown_id() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();

    let error = retry_outbox_impl(&store, &secrets, &[], 4242)
        .await
        .expect_err("unknown outbox id must error");
    assert!(matches!(
        error,
        axomail_core::MailError::Configuration { .. }
    ));
}
