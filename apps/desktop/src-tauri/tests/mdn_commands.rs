//! Integration tests for read receipts (MDN): requesting a receipt on send
//! (the `Disposition-Notification-To` header), and honouring an incoming
//! request via `send_mdn` (disposition-notification report, `$MDNSent` marker
//! and op, and no resend).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;
use std::path::Path;

use axomail_core::{
    AccountId, Address, EmailAddress, MailboxId, MailboxRole, MessageFlags, RemoteMessageId,
    UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::{create_account_impl, list_accounts_impl};
use axomail_desktop_lib::commands::compose::send_message_impl;
use axomail_desktop_lib::commands::mdn::send_mdn_impl;
use axomail_desktop_lib::dto::{AccountSummaryDto, CreateAccountInput, MailboxSummaryDto};
use axomail_desktop_lib::receipts;
use axomail_desktop_lib::state::SharedSecretStore;
use axomail_storage::{NewMessage, PendingOpKind, Store};
use axomail_test_support::{ScriptedServer, Step, TestTlsIdentity};
use common::provider::{StubFactory, stub_engine};
use common::{compose_input, outgoing_account_input, secret_stores, seed_standard_folders};

/// A source message addressed to the account (klaus@theferbers.eu) that
/// requests a read receipt back to the sender.
const SOURCE: &str = concat!(
    "From: Ada Lovelace <ada@example.org>\r\n",
    "To: Klaus <klaus@theferbers.eu>\r\n",
    "Subject: Please confirm\r\n",
    "Message-ID: <orig-1@example.org>\r\n",
    "Disposition-Notification-To: ada@example.org\r\n",
    "\r\n",
    "Did you read this?\r\n",
);

/// A minimal scripted SMTP submission dialog capturing the DATA payload.
fn smtp_steps() -> Vec<Step> {
    vec![
        Step::send_line("220 axomail.test ESMTP ready"),
        Step::expect_prefix("EHLO"),
        Step::send_line("250-axomail.test greets you"),
        Step::send_line("250-AUTH PLAIN"),
        Step::send_line("250 SIZE 52428800"),
        Step::expect_prefix("AUTH"),
        Step::send_line("235 2.7.0 authentication succeeded"),
        Step::expect_prefix("MAIL FROM"),
        Step::send_line("250 2.1.0 sender ok"),
        Step::expect_prefix("RCPT TO"),
        Step::send_line("250 2.1.5 recipient ok"),
        Step::expect_prefix("DATA"),
        Step::send_line("354 end data with <CR><LF>.<CR><LF>"),
        Step::expect_until("."),
        Step::send_line("250 2.0.0 accepted for delivery"),
        Step::expect_prefix("QUIT"),
        Step::send_line("221 2.0.0 bye"),
        Step::Close,
    ]
}

/// Creates an account with the standard folders (so a Sent mailbox exists).
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

fn inbox_mailbox_id(mailboxes: &[MailboxSummaryDto]) -> MailboxId {
    let id = mailboxes
        .iter()
        .find(|mailbox| mailbox.role == MailboxRole::Inbox)
        .expect("inbox exists")
        .id;
    MailboxId::new(id)
}

fn sent_mailbox_id(mailboxes: &[MailboxSummaryDto]) -> MailboxId {
    let id = mailboxes
        .iter()
        .find(|mailbox| mailbox.role == MailboxRole::Sent)
        .expect("sent exists")
        .id;
    MailboxId::new(id)
}

/// Writes `bytes` to `<raw_root>/raw/<message_id>.eml`.
fn cache_raw(raw_root: &Path, message_id: i64, bytes: &str) {
    let dir = raw_root.join("raw");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{message_id}.eml")), bytes).unwrap();
}

/// Inserts the source message (with an IMAP identity) into `mailbox`.
async fn insert_source(store: &Store, mailbox: MailboxId) -> i64 {
    let message = NewMessage {
        mailbox,
        remote_id: Some(RemoteMessageId::Imap {
            uid: 1,
            uid_validity: 1,
        }),
        subject: Some("Please confirm".to_owned()),
        from: vec![Address {
            display_name: Some("Ada Lovelace".to_owned()),
            email: EmailAddress::parse("ada@example.org").unwrap(),
        }],
        to: vec![Address::from_email(
            EmailAddress::parse("klaus@theferbers.eu").unwrap(),
        )],
        cc: vec![],
        bcc: vec![],
        date: Some(UtcTimestamp::from_unix_seconds(1_600_000_000)),
        flags: MessageFlags::default(),
        size_bytes: Some(10),
        preview: None,
        has_raw: true,
    };
    store.messages().insert(&message).await.unwrap().id.value()
}

#[tokio::test]
async fn request_receipt_writes_disposition_notification_to_on_send() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let summary =
        account_with_folders(&store, &secrets, outgoing_account_input(server.port())).await;

    let mut input = compose_input(summary.id, &["bob@example.org"], "With receipt");
    input.request_receipt = true;
    let outcome = send_message_impl(&store, &secrets, &roots, input, false)
        .await
        .unwrap()
        .outcome;
    assert!(outcome.sent);

    let transcript = server.transcripts().into_iter().next().unwrap();
    // The account address (klaus@theferbers.eu) is requested as the receipt
    // target, and the header is written on the message.
    assert!(
        transcript
            .received_text()
            .contains("Disposition-Notification-To: klaus@theferbers.eu"),
        "receipt request header missing from the sent message"
    );
}

#[tokio::test]
async fn no_receipt_header_when_not_requested() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let summary =
        account_with_folders(&store, &secrets, outgoing_account_input(server.port())).await;
    let input = compose_input(summary.id, &["bob@example.org"], "No receipt");
    send_message_impl(&store, &secrets, &roots, input, false)
        .await
        .unwrap();

    let transcript = server.transcripts().into_iter().next().unwrap();
    assert!(
        !transcript
            .received_text()
            .contains("Disposition-Notification-To:"),
        "no receipt should be requested by default"
    );
}

#[tokio::test]
async fn send_mdn_delivers_report_marks_sent_and_does_not_resend() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];
    let raw_root = tempfile::tempdir().unwrap();

    let summary =
        account_with_folders(&store, &secrets, outgoing_account_input(server.port())).await;
    let account = AccountId::new(summary.id);
    let inbox = inbox_mailbox_id(&summary.mailboxes);
    let message_id = insert_source(&store, inbox).await;
    cache_raw(raw_root.path(), message_id, SOURCE);

    let engine = stub_engine(&store);
    let outcome = send_mdn_impl(
        &store,
        &secrets,
        &engine,
        &StubFactory,
        raw_root.path(),
        &roots,
        message_id,
        false,
    )
    .await
    .unwrap();
    assert_eq!(outcome.status, "sent", "the receipt should be delivered");

    // The disposition-notification report reached the wire.
    let transcript = server.transcripts().into_iter().next().unwrap();
    let text = transcript.received_text();
    assert!(
        text.contains("message/disposition-notification"),
        "the report's machine-readable part is missing: {text}"
    );
    assert!(
        text.contains("Disposition:") && text.contains("displayed"),
        "the disposition field is missing: {text}"
    );

    // The receipt was filed in Sent.
    let sent = sent_mailbox_id(&summary.mailboxes);
    let page = store.messages().list_page(sent, None, 10).await.unwrap();
    assert_eq!(page.items.len(), 1, "a copy is filed in Sent");

    // The local marker is set and a `$MDNSent` keyword op is queued.
    assert!(
        receipts::mdn_already_sent(&store, message_id)
            .await
            .unwrap()
    );
    let ops = store
        .pending_ops()
        .claim_next_batch(account, 10)
        .await
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, PendingOpKind::AddKeyword);
    assert!(
        ops[0].payload_json.contains("$MDNSent"),
        "the queued op should carry the $MDNSent keyword: {}",
        ops[0].payload_json
    );

    // A second send is a no-op: it never touches the (now-closed) SMTP server.
    let again = send_mdn_impl(
        &store,
        &secrets,
        &engine,
        &StubFactory,
        raw_root.path(),
        &roots,
        message_id,
        false,
    )
    .await
    .unwrap();
    assert_eq!(again.status, "alreadySent", "a receipt is sent only once");
}

#[tokio::test]
async fn concurrent_send_mdn_sends_exactly_one_receipt() {
    // SECURITY (finding #12): two concurrent send_mdn calls for the same message
    // must not both deliver a receipt. The atomic claim ensures exactly one wins
    // and sends; the other reports it was already sent. The scripted server
    // replays its script for every accepted connection, so a double-send would
    // record two transcripts.
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];
    let raw_root = tempfile::tempdir().unwrap();

    let summary =
        account_with_folders(&store, &secrets, outgoing_account_input(server.port())).await;
    let inbox = inbox_mailbox_id(&summary.mailboxes);
    let message_id = insert_source(&store, inbox).await;
    cache_raw(raw_root.path(), message_id, SOURCE);

    let engine = stub_engine(&store);
    let factory = StubFactory;
    let raw = raw_root.path();
    // Two concurrent send attempts for the same message.
    let (a, b) = tokio::join!(
        send_mdn_impl(
            &store, &secrets, &engine, &factory, raw, &roots, message_id, false
        ),
        send_mdn_impl(
            &store, &secrets, &engine, &factory, raw, &roots, message_id, false
        ),
    );
    let a = a.unwrap();
    let b = b.unwrap();

    let sent = [&a, &b].iter().filter(|o| o.status == "sent").count();
    let already = [&a, &b]
        .iter()
        .filter(|o| o.status == "alreadySent")
        .count();
    assert_eq!(
        sent,
        1,
        "exactly one call sends: {:?}",
        (&a.status, &b.status)
    );
    assert_eq!(
        already,
        1,
        "the other call sees it already sent: {:?}",
        (&a.status, &b.status)
    );
    assert_eq!(
        server.transcripts().len(),
        1,
        "exactly one receipt reaches the wire"
    );
}
