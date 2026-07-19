//! Integration tests for `get_message_detail` using a cached raw `.eml`
//! fixture, so no network is involved.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;

use axomail_core::{AccountId, MailError, MailboxId};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::detail::get_message_detail_impl;
use axomail_desktop_lib::composition::AppProviderFactory;
use axomail_storage::Store;
use axomail_test_support::fixtures;
use common::{sample_message, secret_stores, valid_input};

/// Writes `bytes` to `<raw_root>/raw/<message_id>.eml`, creating the dir.
fn cache_raw(raw_root: &std::path::Path, message_id: i64, bytes: &str) {
    let dir = raw_root.join("raw");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{message_id}.eml")), bytes).unwrap();
}

#[tokio::test]
async fn detail_parses_and_sanitizes_a_cached_message() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let factory = AppProviderFactory::new(secrets.clone(), store.clone());
    let raw_root = tempfile::tempdir().unwrap();

    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = MailboxId::new(summary.mailboxes[0].id);

    // The stored envelope carries a placeholder subject; the DTO must come
    // from parsing the cached bytes, not from the row.
    let envelope = store
        .messages()
        .insert(&sample_message(inbox, "row-subject", Some(1)))
        .await
        .unwrap();
    let message_id = envelope.id.value();
    cache_raw(raw_root.path(), message_id, fixtures::ALTERNATIVE);

    let detail = get_message_detail_impl(&store, &factory, raw_root.path(), message_id)
        .await
        .unwrap();

    assert_eq!(detail.id, message_id);
    assert_eq!(detail.subject.as_deref(), Some("Release notes"));
    assert_eq!(detail.from_name, "Alice Example");
    assert_eq!(detail.from_address, "alice@example.org");
    assert_eq!(detail.to.len(), 1);
    assert_eq!(detail.to[0].address, "bob@example.com");
    assert!(
        detail
            .text_body
            .as_deref()
            .is_some_and(|body| body.contains("Release 2.0"))
    );
    let html = detail.sanitized_html.expect("html alternative present");
    assert!(html.contains("Release"));
    assert!(!html.contains("<script"));
    assert_eq!(detail.remote_blocked, 0);
    assert!(!detail.had_active_content);
    assert!(detail.attachments.is_empty());
}

/// A reply message with threading headers and a plain-text body, used to
/// prove both the DTO exposure and the lazy backfill of migration 0006.
const THREADED_REPLY: &str = concat!(
    "From: Bob Example <bob@example.com>\r\n",
    "To: Alice Example <alice@example.org>\r\n",
    "Subject: Re: Release notes\r\n",
    "Date: Wed, 15 Jul 2026 13:00:00 +0000\r\n",
    "Message-ID: <reply-77@example.com>\r\n",
    "In-Reply-To: <alt-0002@example.org>\r\n",
    "References: <root-0001@example.org> <alt-0002@example.org>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: text/plain; charset=utf-8\r\n",
    "\r\n",
    "Thanks, looks good.\r\n",
);

#[tokio::test]
async fn detail_exposes_threading_and_backfills_the_columns_on_open() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let factory = AppProviderFactory::new(secrets.clone(), store.clone());
    let raw_root = tempfile::tempdir().unwrap();

    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = MailboxId::new(summary.mailboxes[0].id);

    // Insert a row with NULL threading columns (as an envelope-only IMAP
    // sync would), then cache the full raw reply so opening it can parse and
    // backfill the identity.
    let envelope = store
        .messages()
        .insert(&sample_message(inbox, "row-subject", Some(1)))
        .await
        .unwrap();
    let message_id = envelope.id.value();
    assert!(envelope.threading.is_empty(), "starts without threading");
    cache_raw(raw_root.path(), message_id, THREADED_REPLY);

    let detail = get_message_detail_impl(&store, &factory, raw_root.path(), message_id)
        .await
        .unwrap();

    // The DTO exposes the parsed identity (angle brackets stripped).
    assert_eq!(detail.message_id.as_deref(), Some("reply-77@example.com"));
    assert_eq!(detail.in_reply_to, vec!["alt-0002@example.org".to_owned()]);
    assert_eq!(
        detail.references,
        vec![
            "root-0001@example.org".to_owned(),
            "alt-0002@example.org".to_owned()
        ]
    );

    // Opening the message backfilled the stored columns.
    let stored = store
        .messages()
        .get(envelope.id)
        .await
        .unwrap()
        .unwrap()
        .threading;
    assert_eq!(stored.message_id.as_deref(), Some("reply-77@example.com"));
    assert_eq!(stored.references.len(), 2);
    assert_eq!(stored.parent_id(), Some("alt-0002@example.org"));

    // The message is now discoverable by its Message-ID within the account.
    let account_id = summary.id;
    let found = store
        .messages()
        .find_by_message_id(AccountId::new(account_id), "reply-77@example.com")
        .await
        .unwrap();
    assert_eq!(found, Some(envelope.id));
}

#[tokio::test]
async fn detail_without_a_content_source_is_a_configuration_error() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let factory = AppProviderFactory::new(secrets.clone(), store.clone());
    let raw_root = tempfile::tempdir().unwrap();

    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = MailboxId::new(summary.mailboxes[0].id);

    // A local-only message (no remote id) with no cached body has no source.
    let envelope = store
        .messages()
        .insert(&sample_message(inbox, "local only", Some(1)))
        .await
        .unwrap();

    let error = get_message_detail_impl(&store, &factory, raw_root.path(), envelope.id.value())
        .await
        .expect_err("a message with no content source must error");
    assert!(matches!(error, MailError::Configuration { .. }));
}

#[tokio::test]
async fn detail_rejects_an_unknown_message() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let factory = AppProviderFactory::new(secrets.clone(), store.clone());
    let raw_root = tempfile::tempdir().unwrap();

    let error = get_message_detail_impl(&store, &factory, raw_root.path(), 987_654)
        .await
        .expect_err("unknown message id must error");
    assert!(matches!(error, MailError::Configuration { .. }));
}
