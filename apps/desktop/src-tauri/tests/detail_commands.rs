//! Integration tests for `get_message_detail` using a cached raw `.eml`
//! fixture, so no network is involved.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;

use axomail_core::{
    AccountId, Address, EmailAddress, MailError, MailboxId, MessageFlags, RemoteMessageId,
    UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::detail::get_message_detail_impl;
use axomail_desktop_lib::commands::offline::{
    cleanup_offline_cache_impl, set_mailbox_offline_policy_impl,
};
use axomail_desktop_lib::commands::remote_content::set_remote_content_allowed_impl;
use axomail_desktop_lib::composition::AppProviderFactory;
use axomail_storage::{NewMessage, OfflineMode, Store};
use axomail_test_support::fixtures;
use common::provider::CannedFactory;
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

    let detail =
        get_message_detail_impl(&store, &factory, raw_root.path(), message_id, None, false)
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

    let detail =
        get_message_detail_impl(&store, &factory, raw_root.path(), message_id, None, false)
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

    let error = get_message_detail_impl(
        &store,
        &factory,
        raw_root.path(),
        envelope.id.value(),
        None,
        false,
    )
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

    let error = get_message_detail_impl(&store, &factory, raw_root.path(), 987_654, None, false)
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

    let detail =
        get_message_detail_impl(&store, &factory, raw_root.path(), message_id, None, false)
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

    let detail = get_message_detail_impl(
        &store,
        &factory,
        raw_root.path(),
        message_id,
        Some(true),
        false,
    )
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
    let allowed =
        get_message_detail_impl(&store, &factory, raw_root.path(), message_id, None, false)
            .await
            .unwrap();
    assert!(
        allowed.remote_allowed,
        "an allowed sender loads remote images"
    );
    assert_eq!(allowed.remote_blocked_count, 0);

    // An explicit block override wins over the stored allow decision.
    let forced_block = get_message_detail_impl(
        &store,
        &factory,
        raw_root.path(),
        message_id,
        Some(false),
        false,
    )
    .await
    .unwrap();
    assert!(!forced_block.remote_allowed);
    assert_eq!(forced_block.remote_blocked_count, 1);

    // Clearing the sender returns to the blocked default.
    set_remote_content_allowed_impl(&store, SCAM_SENDER, false)
        .await
        .unwrap();
    let blocked =
        get_message_detail_impl(&store, &factory, raw_root.path(), message_id, None, false)
            .await
            .unwrap();
    assert!(!blocked.remote_allowed);
    assert_eq!(blocked.remote_blocked_count, 1);
}

/// Inserts a remote-backed (IMAP) message with no cached body, so opening it
/// exercises the reading pane's on-demand fetch path.
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

#[tokio::test]
async fn lazy_fetch_caches_body_sets_has_raw_and_cleanup_purges_it() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let raw_root = tempfile::tempdir().unwrap();
    let factory = CannedFactory {
        bytes: fixtures::ALTERNATIVE.as_bytes().to_vec(),
    };

    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let mailbox = MailboxId::new(summary.mailboxes[0].id);
    let message_id = insert_remote_message(&store, mailbox).await;

    // Online + uncached: opening the message fetches and caches its body.
    let detail =
        get_message_detail_impl(&store, &factory, raw_root.path(), message_id, None, false)
            .await
            .unwrap();
    assert!(!detail.offline_unavailable);
    assert_eq!(detail.subject.as_deref(), Some("Release notes"));

    // The lazy fetch flipped `has_raw`, so with a `none` offline policy the
    // cached body is outside the window: cleanup enumerates and purges exactly
    // it. Without the fix `has_raw` stays 0 and cleanup would find nothing.
    set_mailbox_offline_policy_impl(&store, mailbox.value(), OfflineMode::None, 30)
        .await
        .unwrap();
    let purged = cleanup_offline_cache_impl(&store, raw_root.path(), mailbox.value())
        .await
        .unwrap();
    assert_eq!(
        purged, 1,
        "the lazily-fetched body was tracked as cached and purged"
    );
}

#[tokio::test]
async fn offline_uncached_message_returns_placeholder_without_fetching() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let raw_root = tempfile::tempdir().unwrap();
    // A provider that WOULD serve a body; the offline gate must short-circuit
    // before any fetch happens.
    let factory = CannedFactory {
        bytes: fixtures::ALTERNATIVE.as_bytes().to_vec(),
    };

    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let mailbox = MailboxId::new(summary.mailboxes[0].id);
    let message_id = insert_remote_message(&store, mailbox).await;

    // Offline + uncached: an honest placeholder, built from the stored
    // envelope, with no body and nothing fetched or cached.
    let detail = get_message_detail_impl(&store, &factory, raw_root.path(), message_id, None, true)
        .await
        .unwrap();
    assert!(
        detail.offline_unavailable,
        "an uncached body is unavailable while offline"
    );
    assert!(detail.text_body.is_none());
    assert!(detail.sanitized_html.is_none());
    assert_eq!(detail.from_address, "ada@example.org", "header still shown");

    // Nothing was written to the raw cache and `has_raw` stayed 0.
    let cached = raw_root
        .path()
        .join("raw")
        .join(format!("{message_id}.eml"));
    assert!(!cached.exists(), "offline must not fetch or cache the body");
    set_mailbox_offline_policy_impl(&store, mailbox.value(), OfflineMode::None, 30)
        .await
        .unwrap();
    let purged = cleanup_offline_cache_impl(&store, raw_root.path(), mailbox.value())
        .await
        .unwrap();
    assert_eq!(purged, 0, "no cached body exists to purge");
}
