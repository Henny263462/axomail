//! Integration tests for the attachment / raw-message export commands
//! (Track 3.6), driven off cached raw `.eml` fixtures so no network runs.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;

use axomail_core::{
    Address, EmailAddress, MailError, MailboxId, MessageFlags, RemoteMessageId, UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::attachments::{
    get_message_headers_impl, get_message_source_impl, save_all_attachments_impl,
    save_attachment_impl, save_message_eml_impl,
};
use axomail_desktop_lib::composition::AppProviderFactory;
use axomail_storage::{NewMessage, Store};
use axomail_test_support::fixtures;
use common::provider::CannedFactory;
use common::{sample_message, secret_stores, valid_input};

/// The single attachment in [`fixtures::MIXED_WITH_ATTACHMENT`], base64-decoded.
const INVOICE_BYTES: &[u8] = b"Invoice #1234\nTotal due: EUR 42.00\n";

/// A `multipart/mixed` message whose attachment declares a path-traversal
/// filename, to prove save-all sanitizes the name before joining it.
const HOSTILE_NAME: &str = concat!(
    "From: Mallory <mallory@example.org>\r\n",
    "To: Klaus <klaus@theferbers.eu>\r\n",
    "Subject: Sneaky\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: multipart/mixed; boundary=\"b\"\r\n",
    "\r\n",
    "--b\r\n",
    "Content-Type: text/plain\r\n",
    "\r\n",
    "See attached.\r\n",
    "\r\n",
    "--b\r\n",
    "Content-Type: text/plain; name=\"../../escape.txt\"\r\n",
    "Content-Transfer-Encoding: base64\r\n",
    "Content-Disposition: attachment; filename=\"../../escape.txt\"\r\n",
    "\r\n",
    "cGF5bG9hZA==\r\n", // "payload"
    "\r\n",
    "--b--\r\n",
);

/// Inserts a remote-backed (IMAP) message with no cached body, so an export
/// exercises the on-demand / offline-gate path.
async fn insert_remote_message(store: &Store, mailbox: MailboxId) -> i64 {
    let message = NewMessage {
        mailbox,
        remote_id: Some(RemoteMessageId::Imap {
            uid: 1,
            uid_validity: 1,
        }),
        subject: Some("row-subject".to_owned()),
        from: vec![Address::from_email(
            EmailAddress::parse("ada@example.org").unwrap(),
        )],
        to: vec![],
        cc: vec![],
        bcc: vec![],
        date: Some(UtcTimestamp::from_unix_seconds(1)),
        flags: MessageFlags::default(),
        size_bytes: Some(10),
        preview: None,
        has_raw: false,
    };
    store.messages().insert(&message).await.unwrap().id.value()
}

/// Writes `bytes` to `<raw_root>/raw/<message_id>.eml`, creating the dir.
fn cache_raw(raw_root: &std::path::Path, message_id: i64, bytes: &str) {
    let dir = raw_root.join("raw");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{message_id}.eml")), bytes).unwrap();
}

/// Seeds an account + a cached message, returning `(store, factory, message_id)`
/// with the message's raw bytes set to `raw`.
async fn seed(raw_root: &std::path::Path, raw: &str) -> (Store, AppProviderFactory, i64) {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let factory = AppProviderFactory::new(secrets.clone(), store.clone());
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = MailboxId::new(summary.mailboxes[0].id);
    let envelope = store
        .messages()
        .insert(&sample_message(inbox, "row-subject", Some(1)))
        .await
        .unwrap();
    let message_id = envelope.id.value();
    cache_raw(raw_root, message_id, raw);
    (store, factory, message_id)
}

#[tokio::test]
async fn save_attachment_writes_the_exact_bytes_to_the_chosen_path() {
    let raw_root = tempfile::tempdir().unwrap();
    let (store, factory, message_id) = seed(raw_root.path(), fixtures::MIXED_WITH_ATTACHMENT).await;

    let out_dir = tempfile::tempdir().unwrap();
    let dest = out_dir.path().join("saved-invoice.txt");
    let saved = save_attachment_impl(
        &store,
        &factory,
        raw_root.path(),
        false,
        message_id,
        0,
        &dest,
    )
    .await
    .unwrap();

    assert_eq!(saved.filename, "saved-invoice.txt");
    assert_eq!(saved.bytes as usize, INVOICE_BYTES.len());
    assert_eq!(fs::read(&dest).unwrap(), INVOICE_BYTES);
}

#[tokio::test]
async fn save_attachment_rejects_an_out_of_range_index() {
    let raw_root = tempfile::tempdir().unwrap();
    let (store, factory, message_id) = seed(raw_root.path(), fixtures::MIXED_WITH_ATTACHMENT).await;
    let dest = tempfile::tempdir().unwrap().path().join("x.txt");

    let error = save_attachment_impl(
        &store,
        &factory,
        raw_root.path(),
        false,
        message_id,
        7,
        &dest,
    )
    .await
    .expect_err("index 7 is out of range");
    assert!(matches!(error, MailError::Configuration { .. }));
    assert!(!dest.exists(), "nothing is written for a bad index");
}

#[tokio::test]
async fn save_all_sanitizes_names_and_keeps_them_inside_the_directory() {
    let raw_root = tempfile::tempdir().unwrap();
    let (store, factory, message_id) = seed(raw_root.path(), HOSTILE_NAME).await;

    let out_dir = tempfile::tempdir().unwrap();
    let saved = save_all_attachments_impl(
        &store,
        &factory,
        raw_root.path(),
        false,
        message_id,
        out_dir.path(),
    )
    .await
    .unwrap();

    assert_eq!(saved.len(), 1);
    let name = &saved[0].filename;
    // The path separators are gone, so the name is a single component that
    // cannot traverse out of the chosen directory (the `..` dot runs are inert
    // without a following separator).
    assert!(
        !name.contains('/') && !name.contains('\\'),
        "no separators: {name}"
    );
    assert_ne!(name, "..");
    // The file landed inside the chosen directory with the real bytes.
    let written = out_dir.path().join(name);
    assert_eq!(fs::read(&written).unwrap(), b"payload");
    // Exactly one file was created, inside the directory — nothing escaped.
    let entries: Vec<_> = fs::read_dir(out_dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 1, "one file, inside the directory");
    assert!(!out_dir.path().parent().unwrap().join("escape.txt").exists());
}

#[tokio::test]
async fn save_all_deduplicates_colliding_names() {
    // Two attachments with the same declared name must not overwrite each other.
    const TWO_SAME: &str = concat!(
        "From: a@example.org\r\nTo: b@example.org\r\nSubject: dup\r\n",
        "MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"b\"\r\n\r\n",
        "--b\r\nContent-Type: text/plain; name=\"note.txt\"\r\n",
        "Content-Disposition: attachment; filename=\"note.txt\"\r\n\r\none\r\n\r\n",
        "--b\r\nContent-Type: text/plain; name=\"note.txt\"\r\n",
        "Content-Disposition: attachment; filename=\"note.txt\"\r\n\r\ntwo\r\n\r\n",
        "--b--\r\n",
    );
    let raw_root = tempfile::tempdir().unwrap();
    let (store, factory, message_id) = seed(raw_root.path(), TWO_SAME).await;
    let out_dir = tempfile::tempdir().unwrap();

    let saved = save_all_attachments_impl(
        &store,
        &factory,
        raw_root.path(),
        false,
        message_id,
        out_dir.path(),
    )
    .await
    .unwrap();
    assert_eq!(saved.len(), 2);
    assert_ne!(
        saved[0].filename, saved[1].filename,
        "names diverge on collision"
    );
    assert_eq!(
        fs::read_dir(out_dir.path()).unwrap().count(),
        2,
        "both files exist"
    );
}

#[tokio::test]
async fn view_source_returns_the_full_raw_message() {
    let raw_root = tempfile::tempdir().unwrap();
    let (store, factory, message_id) = seed(raw_root.path(), fixtures::PLAIN).await;

    let source = get_message_source_impl(&store, &factory, raw_root.path(), false, message_id)
        .await
        .unwrap();
    assert_eq!(source, fixtures::PLAIN);
    assert!(source.contains("Subject: Lunch on Thursday"));
    assert!(source.contains("Are we still on for lunch"));
}

#[tokio::test]
async fn all_headers_returns_only_the_header_block() {
    let raw_root = tempfile::tempdir().unwrap();
    let (store, factory, message_id) = seed(raw_root.path(), fixtures::PLAIN).await;

    let headers = get_message_headers_impl(&store, &factory, raw_root.path(), false, message_id)
        .await
        .unwrap();
    assert!(headers.contains("Message-ID: <plain-0001@example.org>"));
    assert!(headers.contains("Content-Transfer-Encoding: 7bit"));
    // The body is excluded (it follows the first blank line).
    assert!(!headers.contains("Are we still on for lunch"));
}

#[tokio::test]
async fn save_as_eml_writes_the_whole_raw_message() {
    let raw_root = tempfile::tempdir().unwrap();
    let (store, factory, message_id) = seed(raw_root.path(), fixtures::ALTERNATIVE).await;

    let out_dir = tempfile::tempdir().unwrap();
    let dest = out_dir.path().join("Release notes.eml");
    let written =
        save_message_eml_impl(&store, &factory, raw_root.path(), false, message_id, &dest)
            .await
            .unwrap();

    assert_eq!(written as usize, fixtures::ALTERNATIVE.len());
    assert_eq!(fs::read_to_string(&dest).unwrap(), fixtures::ALTERNATIVE);
}

#[tokio::test]
async fn export_offline_without_cache_reports_not_downloaded_and_never_fetches() {
    // A remote-backed message with no cached body must not fetch while offline.
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let raw_root = tempfile::tempdir().unwrap();
    let factory = CannedFactory {
        bytes: fixtures::PLAIN.as_bytes().to_vec(),
    };
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let mailbox = MailboxId::new(summary.mailboxes[0].id);
    let message_id = insert_remote_message(&store, mailbox).await;

    let error = get_message_source_impl(&store, &factory, raw_root.path(), true, message_id)
        .await
        .expect_err("offline + uncached must not fetch");
    assert!(matches!(error, MailError::Configuration { .. }));
    let cached = raw_root
        .path()
        .join("raw")
        .join(format!("{message_id}.eml"));
    assert!(!cached.exists(), "offline export must not cache a body");
}
