//! Integration tests for `get_message_detail` using a cached raw `.eml`
//! fixture, so no network is involved.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;

use axomail_core::{MailError, MailboxId};
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
