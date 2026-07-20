//! Integration tests for the mbox export / import commands (Track 4.6),
//! driven off cached raw `.eml` fixtures so no network runs.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;
use std::path::Path;

use axomail_core::{
    AccountId, Address, EmailAddress, MailError, MailboxId, MailboxRole, MessageFlags,
    RemoteMessageId, UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::mbox::{export_folder_mbox_impl, import_mbox_impl};
use axomail_desktop_lib::composition::AppProviderFactory;
use axomail_storage::{NewMessage, Store, raw_message_path};
use axomail_test_support::fixtures;
use common::{sample_message, secret_stores, seed_standard_folders, valid_input};

/// Opens an in-memory store, creates an IMAP account with the standard folders,
/// and returns `(store, factory, inbox, archive)`.
async fn setup() -> (Store, AppProviderFactory, MailboxId, MailboxId) {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let factory = AppProviderFactory::new(secrets.clone(), store.clone());
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let account = AccountId::new(summary.id);
    seed_standard_folders(&store, account).await;
    let mailboxes = store.mailboxes().list(account).await.unwrap();
    let role = |role: MailboxRole| mailboxes.iter().find(|m| m.role == role).unwrap().id;
    (
        store,
        factory,
        role(MailboxRole::Inbox),
        role(MailboxRole::Archive),
    )
}

/// Inserts a message into `mailbox` and caches `raw` as its body.
async fn insert_cached(
    store: &Store,
    raw_root: &Path,
    mailbox: MailboxId,
    subject: &str,
    raw: &[u8],
) {
    let envelope = store
        .messages()
        .insert(&sample_message(mailbox, subject, Some(1)))
        .await
        .unwrap();
    let path = raw_message_path(raw_root, envelope.id.value());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, raw).unwrap();
}

#[tokio::test]
async fn export_frames_messages_with_postmarks_and_escapes_from_lines() {
    let raw_root = tempfile::tempdir().unwrap();
    let (store, factory, inbox, _archive) = setup().await;
    // A body line beginning "From " must be `>From`-escaped; the "From:" header
    // must not (it is not a postmark).
    let raw = b"From: a@b.c\r\nSubject: hi\r\n\r\nFrom the desk of Alice\r\nbody\r\n";
    insert_cached(&store, raw_root.path(), inbox, "hi", raw).await;

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("inbox.mbox");
    let result = export_folder_mbox_impl(
        &store,
        &factory,
        raw_root.path(),
        false,
        inbox.value(),
        &dest,
    )
    .await
    .unwrap();

    assert_eq!(result.exported, 1);
    assert_eq!(result.skipped, 0);
    let content = fs::read_to_string(&dest).unwrap();
    assert!(content.starts_with("From "), "starts with a postmark");
    assert!(
        content.contains("\nFrom: a@b.c\r\n"),
        "the From header is untouched"
    );
    assert!(
        content.contains("\n>From the desk of Alice\r\n"),
        "the body From line is escaped"
    );
    assert!(content.ends_with("\n\n"), "trailing blank-line separator");
}

#[tokio::test]
async fn export_then_import_round_trips_messages() {
    let raw_root = tempfile::tempdir().unwrap();
    let (store, factory, inbox, archive) = setup().await;
    insert_cached(
        &store,
        raw_root.path(),
        inbox,
        "one",
        fixtures::PLAIN.as_bytes(),
    )
    .await;
    insert_cached(
        &store,
        raw_root.path(),
        inbox,
        "two",
        fixtures::ALTERNATIVE.as_bytes(),
    )
    .await;

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("inbox.mbox");
    let exported = export_folder_mbox_impl(
        &store,
        &factory,
        raw_root.path(),
        false,
        inbox.value(),
        &dest,
    )
    .await
    .unwrap();
    assert_eq!(exported.exported, 2);

    let imported = import_mbox_impl(&store, raw_root.path(), archive.value(), &dest)
        .await
        .unwrap();
    assert_eq!(imported.imported, 2);

    let page = store.messages().list_page(archive, None, 50).await.unwrap();
    assert_eq!(page.items.len(), 2);
    let raws: Vec<Vec<u8>> = page
        .items
        .iter()
        .map(|env| fs::read(raw_message_path(raw_root.path(), env.id.value())).unwrap())
        .collect();
    // Order-independent: both originals survive byte-for-byte.
    assert!(raws.iter().any(|r| r == fixtures::PLAIN.as_bytes()));
    assert!(raws.iter().any(|r| r == fixtures::ALTERNATIVE.as_bytes()));
}

#[tokio::test]
async fn imports_a_single_eml_file_as_one_message() {
    let raw_root = tempfile::tempdir().unwrap();
    let (store, _factory, inbox, _archive) = setup().await;
    let src_dir = tempfile::tempdir().unwrap();
    let src = src_dir.path().join("message.eml");
    fs::write(&src, fixtures::PLAIN).unwrap();

    let result = import_mbox_impl(&store, raw_root.path(), inbox.value(), &src)
        .await
        .unwrap();
    assert_eq!(result.imported, 1);

    let page = store.messages().list_page(inbox, None, 50).await.unwrap();
    assert_eq!(page.items.len(), 1);
    let bytes = fs::read(raw_message_path(raw_root.path(), page.items[0].id.value())).unwrap();
    assert_eq!(bytes, fixtures::PLAIN.as_bytes());
    // The envelope was parsed from the .eml headers.
    assert_eq!(page.items[0].subject.as_deref(), Some("Lunch on Thursday"));
}

#[tokio::test]
async fn export_skips_uncached_remote_messages_when_offline() {
    let raw_root = tempfile::tempdir().unwrap();
    let (store, factory, inbox, _archive) = setup().await;
    // A remote-backed message with no cached body: offline export must not fetch
    // it, so it is skipped and counted, never fatal.
    let message = NewMessage {
        mailbox: inbox,
        remote_id: Some(RemoteMessageId::Imap {
            uid: 1,
            uid_validity: 1,
        }),
        subject: Some("remote".to_owned()),
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
    store.messages().insert(&message).await.unwrap();

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("inbox.mbox");
    let result = export_folder_mbox_impl(
        &store,
        &factory,
        raw_root.path(),
        true,
        inbox.value(),
        &dest,
    )
    .await
    .unwrap();
    assert_eq!(result.exported, 0);
    assert_eq!(result.skipped, 1);
}

#[tokio::test]
async fn export_and_import_reject_an_unknown_folder() {
    let raw_root = tempfile::tempdir().unwrap();
    let (store, factory, _inbox, _archive) = setup().await;
    let out = tempfile::tempdir().unwrap();
    let missing = 999_999;

    let export_error = export_folder_mbox_impl(
        &store,
        &factory,
        raw_root.path(),
        false,
        missing,
        &out.path().join("x.mbox"),
    )
    .await
    .expect_err("unknown folder");
    assert!(matches!(export_error, MailError::Configuration { .. }));

    let src = out.path().join("in.eml");
    fs::write(&src, fixtures::PLAIN).unwrap();
    let import_error = import_mbox_impl(&store, raw_root.path(), missing, &src)
        .await
        .expect_err("unknown folder");
    assert!(matches!(import_error, MailError::Configuration { .. }));
}
