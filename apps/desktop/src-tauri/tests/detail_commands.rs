//! Integration tests for `get_message_detail` using a cached raw `.eml`
//! fixture, so no network is involved.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;

use axomail_core::{MailError, MailboxId};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::detail::get_message_detail_impl;
use axomail_desktop_lib::commands::remote_content::set_remote_content_allowed_impl;
use axomail_desktop_lib::composition::AppProviderFactory;
use axomail_storage::Store;
use axomail_test_support::fixtures;
use common::{sample_message, secret_stores, valid_input};

/// An HTML message from a known sender carrying a remote tracking image and a
/// deceptive link whose visible text names a site but points at a raw IP.
const REMOTE_AND_SCAM: &str = concat!(
    "From: Bank Security <security@bank.example>\r\n",
    "To: Klaus <klaus@theferbers.eu>\r\n",
    "Subject: Action required\r\n",
    "Date: Sat, 18 Jul 2026 09:00:00 +0000\r\n",
    "Message-ID: <scam-9001@bank.example>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: text/html; charset=utf-8\r\n",
    "Content-Transfer-Encoding: 7bit\r\n",
    "\r\n",
    "<html><body>",
    "<p>Verify now at <a href=\"http://192.168.5.5/login\">secure-bank.com</a>.</p>",
    "<img src=\"https://tracker.example/pixel.gif\" alt=\"\">",
    "</body></html>\r\n",
);

/// The sender address of [`REMOTE_AND_SCAM`], as stored on the allow list.
const SCAM_SENDER: &str = "security@bank.example";

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

    let detail = get_message_detail_impl(&store, &factory, raw_root.path(), message_id, None)
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
    assert_eq!(detail.remote_blocked_count, 0);
    assert!(!detail.remote_allowed);
    assert!(detail.scam_flags.is_empty());
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

    let error =
        get_message_detail_impl(&store, &factory, raw_root.path(), envelope.id.value(), None)
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

    let error = get_message_detail_impl(&store, &factory, raw_root.path(), 987_654, None)
        .await
        .expect_err("unknown message id must error");
    assert!(matches!(error, MailError::Configuration { .. }));
}

/// Inserts [`REMOTE_AND_SCAM`] and caches its raw bytes, returning its id.
async fn seed_remote_and_scam(store: &Store, raw_root: &std::path::Path) -> i64 {
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = MailboxId::new(summary.mailboxes[0].id);
    let envelope = store
        .messages()
        .insert(&sample_message(inbox, "row-subject", Some(1)))
        .await
        .unwrap();
    let message_id = envelope.id.value();
    cache_raw(raw_root, message_id, REMOTE_AND_SCAM);
    message_id
}

#[tokio::test]
async fn remote_images_are_blocked_by_default_and_scam_flags_are_surfaced() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let factory = AppProviderFactory::new(secrets.clone(), store.clone());
    let raw_root = tempfile::tempdir().unwrap();
    let message_id = seed_remote_and_scam(&store, raw_root.path()).await;

    let detail = get_message_detail_impl(&store, &factory, raw_root.path(), message_id, None)
        .await
        .unwrap();

    // Default policy: the remote image is blocked and counted, not loaded.
    assert_eq!(detail.remote_blocked_count, 1);
    assert!(!detail.remote_allowed);
    let html = detail.sanitized_html.clone().expect("html body present");
    assert!(
        !html.contains("tracker.example"),
        "remote image must be blocked"
    );

    // Both scam heuristics fire: a raw-IP host and deceptive link text.
    let kinds: Vec<&str> = detail.scam_flags.iter().map(|f| f.kind.as_str()).collect();
    assert!(kinds.contains(&"ipAddressHost"), "got {kinds:?}");
    assert!(kinds.contains(&"deceptiveLink"), "got {kinds:?}");
    assert!(detail.scam_flags.iter().all(|f| !f.detail.is_empty()));
}

#[tokio::test]
async fn allow_once_override_shows_remote_images_without_persisting() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let factory = AppProviderFactory::new(secrets.clone(), store.clone());
    let raw_root = tempfile::tempdir().unwrap();
    let message_id = seed_remote_and_scam(&store, raw_root.path()).await;

    let detail = get_message_detail_impl(&store, &factory, raw_root.path(), message_id, Some(true))
        .await
        .unwrap();

    assert!(detail.remote_allowed);
    assert_eq!(
        detail.remote_blocked_count, 0,
        "the loaded image is not blocked"
    );
    let html = detail.sanitized_html.expect("html body present");
    assert!(html.contains("https://tracker.example/pixel.gif"));

    // Allow-once did not persist: the sender is still blocked by default.
    assert!(
        !store
            .remote_content()
            .is_allowed(SCAM_SENDER)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn allowed_sender_is_consulted_and_override_false_still_wins() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let factory = AppProviderFactory::new(secrets.clone(), store.clone());
    let raw_root = tempfile::tempdir().unwrap();
    let message_id = seed_remote_and_scam(&store, raw_root.path()).await;

    // Persist the sender through the command impl, then load with no override.
    set_remote_content_allowed_impl(&store, SCAM_SENDER, true)
        .await
        .unwrap();
    let allowed = get_message_detail_impl(&store, &factory, raw_root.path(), message_id, None)
        .await
        .unwrap();
    assert!(
        allowed.remote_allowed,
        "an allowed sender loads remote images"
    );
    assert_eq!(allowed.remote_blocked_count, 0);

    // An explicit block override wins over the stored allow decision.
    let forced_block =
        get_message_detail_impl(&store, &factory, raw_root.path(), message_id, Some(false))
            .await
            .unwrap();
    assert!(!forced_block.remote_allowed);
    assert_eq!(forced_block.remote_blocked_count, 1);

    // Clearing the sender returns to the blocked default.
    set_remote_content_allowed_impl(&store, SCAM_SENDER, false)
        .await
        .unwrap();
    let blocked = get_message_detail_impl(&store, &factory, raw_root.path(), message_id, None)
        .await
        .unwrap();
    assert!(!blocked.remote_allowed);
    assert_eq!(blocked.remote_blocked_count, 1);
}
