//! Integration tests for the send/outbox commands, driven against a scripted
//! TLS SMTP server from `axomail-test-support`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{AccountId, MailboxId, MailboxRole};
use axomail_desktop_lib::commands::accounts::{create_account_impl, list_accounts_impl};
use axomail_desktop_lib::commands::compose::{
    list_outbox_impl, retry_outbox_impl, send_message_impl,
};
use axomail_desktop_lib::dto::{AccountSummaryDto, ComposeAttachmentDto, CreateAccountInput};
use axomail_desktop_lib::state::SharedSecretStore;
use axomail_storage::{NewOutbox, OutboxId, OutboxState, Store};
use axomail_test_support::{ScriptedServer, Step, TestTlsIdentity};
use base64::Engine;
use common::{compose_input, outgoing_account_input, secret_stores, seed_standard_folders};

/// Creates an account and adds the standard folders (IMAP seeds only the
/// inbox), returning the re-listed summary so it carries a Sent mailbox.
async fn account_with_folders(
    store: &Store,
    secrets: &SharedSecretStore,
    input: CreateAccountInput,
) -> AccountSummaryDto {
    let created = create_account_impl(store, secrets, input).await.unwrap();
    seed_standard_folders(store, AccountId::new(created.id)).await;
    list_accounts_impl(store)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
}

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

    let summary =
        account_with_folders(&store, &secrets, outgoing_account_input(server.port())).await;

    let input = compose_input(summary.id, &["bob@example.org"], "Meeting notes");
    let outcome = send_message_impl(&store, &secrets, &roots, input, false)
        .await
        .unwrap()
        .outcome;

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

    let summary =
        account_with_folders(&store, &secrets, outgoing_account_input(server.port())).await;

    let input = compose_input(summary.id, &["bob@example.org"], "Undeliverable");
    let outcome = send_message_impl(&store, &secrets, &roots, input, false)
        .await
        .unwrap()
        .outcome;

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
        .unwrap()
        .outcome;
    assert!(outcome.sent);

    let got = store.outbox().get(record.id).await.unwrap().unwrap();
    assert_eq!(got.state, OutboxState::Sent);
    assert_eq!(got.attempts, 1);

    // Retrying a sent row reports success but does not resubmit.
    let again = retry_outbox_impl(&store, &secrets, &roots, record.id.value())
        .await
        .unwrap()
        .outcome;
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

#[tokio::test]
async fn send_message_with_html_and_attachment_builds_multipart() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_steps(true, "250 2.1.5 ok"), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let summary =
        account_with_folders(&store, &secrets, outgoing_account_input(server.port())).await;

    // A real file on disk is the attachment source (dialog picks path, Rust
    // reads bytes). A traversal-shaped basename must land sanitized in the MIME.
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("quarterly.pdf");
    std::fs::write(&file_path, b"%PDF-1.7 attachment payload").unwrap();

    let mut input = compose_input(summary.id, &["bob@example.org"], "Rich message");
    input.text_body = "Plain text alternative.".to_owned();
    // The HTML carries a hostile <script>; the send path must sanitize it out.
    input.html_body = Some("<p>Hello in <b>HTML</b></p><script>alert(1)</script>".to_owned());
    input.attachments = vec![file_path.to_str().unwrap().to_owned()];

    let outcome = send_message_impl(&store, &secrets, &roots, input, false)
        .await
        .unwrap()
        .outcome;
    assert!(outcome.sent, "rich send should succeed");

    let transcript = server.transcripts().into_iter().next().unwrap();
    let received = transcript.received_text();
    assert!(
        received.contains("multipart/alternative"),
        "expected a text+html alternative part"
    );
    assert!(
        received.contains("multipart/mixed"),
        "expected a mixed part wrapping the attachment"
    );
    assert!(received.contains("Plain text alternative."));
    assert!(received.contains("<b>HTML</b>"));
    // The attachment name (base64 content aside) appears in the part headers.
    assert!(
        received.contains("quarterly.pdf"),
        "attachment filename should be present"
    );
    // The injected script was stripped by the server-side sanitizer.
    assert!(
        !received.contains("alert(1)"),
        "hostile script must not reach the wire: {received}"
    );
}

/// A forwarded message (inline base64 `message/rfc822`) and a user-picked file
/// (path read by Rust) combine into a single `multipart/mixed` tree — the two
/// attachment transports unified across tracks 3.1 and 3.5.
#[tokio::test]
async fn forward_with_picked_file_builds_one_mixed_tree() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_steps(true, "250 2.1.5 ok"), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let summary =
        account_with_folders(&store, &secrets, outgoing_account_input(server.port())).await;

    // A user-picked file: the dialog gives a path, Rust reads the bytes.
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("agenda.pdf");
    std::fs::write(&file_path, b"%PDF-1.7 picked file payload").unwrap();

    // A forwarded message rides as an inline base64 message/rfc822 part.
    let original =
        b"From: carol@example.net\r\nSubject: Original\r\n\r\nBody of forwarded note.\r\n";
    let forwarded = ComposeAttachmentDto {
        filename: "Original.eml".to_owned(),
        mime_type: "message/rfc822".to_owned(),
        content_base64: base64::engine::general_purpose::STANDARD.encode(original),
    };

    let mut input = compose_input(summary.id, &["bob@example.org"], "Fwd: Original");
    input.text_body = "See the attached message and file.".to_owned();
    input.attachments = vec![file_path.to_str().unwrap().to_owned()];
    input.forwarded_attachments = vec![forwarded];

    let outcome = send_message_impl(&store, &secrets, &roots, input, false)
        .await
        .unwrap()
        .outcome;
    assert!(outcome.sent, "forward + picked file should send");

    let transcript = server.transcripts().into_iter().next().unwrap();
    let received = transcript.received_text();
    // One mixed tree carrying BOTH the picked file and the forwarded message.
    assert!(
        received.contains("multipart/mixed"),
        "expected a mixed wrapper: {received}"
    );
    assert!(
        received.contains("agenda.pdf"),
        "picked-file part missing: {received}"
    );
    assert!(
        received.contains("message/rfc822"),
        "forwarded rfc822 part missing: {received}"
    );
    assert!(
        received.contains("Original.eml"),
        "forwarded filename missing: {received}"
    );
}

#[tokio::test]
async fn scheduled_send_with_attachment_is_rejected_before_queueing() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, outgoing_account_input(2525))
        .await
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("deck.pdf");
    std::fs::write(&file_path, b"payload").unwrap();

    let mut input = compose_input(summary.id, &["bob@example.org"], "Later with file");
    input.attachments = vec![file_path.to_str().unwrap().to_owned()];
    // Far in the future so it is treated as a scheduled send.
    input.scheduled_at_unix = Some(i64::MAX);

    let error = send_message_impl(&store, &secrets, &[], input, false)
        .await
        .expect_err("a scheduled send with an attachment must be rejected");
    assert!(matches!(
        error,
        axomail_core::MailError::Configuration { .. }
    ));
    // Nothing was queued: the rejection happened before any outbox row.
    assert!(list_outbox_impl(&store).await.unwrap().is_empty());
}

#[tokio::test]
async fn offline_send_with_attachment_is_rejected() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, outgoing_account_input(2525))
        .await
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("photo.png");
    std::fs::write(&file_path, b"\x89PNG payload").unwrap();

    let mut input = compose_input(summary.id, &["bob@example.org"], "Offline with file");
    input.attachments = vec![file_path.to_str().unwrap().to_owned()];

    let error = send_message_impl(&store, &secrets, &[], input, true)
        .await
        .expect_err("an offline send with an attachment must be rejected");
    assert!(matches!(
        error,
        axomail_core::MailError::Configuration { .. }
    ));
    assert!(list_outbox_impl(&store).await.unwrap().is_empty());
}

#[tokio::test]
async fn html_only_message_may_be_scheduled_as_text() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, outgoing_account_input(2525))
        .await
        .unwrap();

    let mut input = compose_input(summary.id, &["bob@example.org"], "Later, formatted");
    input.html_body = Some("<p>Bold <b>plans</b></p>".to_owned());
    input.scheduled_at_unix = Some(i64::MAX);

    // No attachment: an HTML-only message degrades to its persisted text
    // alternative and is accepted as a scheduled send.
    let outcome = send_message_impl(&store, &secrets, &[], input, false)
        .await
        .unwrap()
        .outcome;
    assert!(!outcome.sent);
    assert_eq!(outcome.scheduled_at_unix, Some(i64::MAX));
    let items = list_outbox_impl(&store).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].state, "pending");
}
