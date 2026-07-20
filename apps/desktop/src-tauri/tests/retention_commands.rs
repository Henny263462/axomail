//! Integration tests for the per-mailbox retention commands: reading/writing
//! the policy and applying it through the two safe dispositions.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use axomail_core::{
    AccountId, Address, EmailAddress, MailboxId, MailboxName, MailboxRole, MessageFlags, MessageId,
    RemoteMessageId, UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::retention::{
    apply_folder_retention_impl, get_folder_retention_impl, set_folder_retention_impl,
};
use axomail_desktop_lib::composition::AppProviderFactory;
use axomail_desktop_lib::dto::RetentionPolicyInput;
use axomail_desktop_lib::state::{AppSyncEngine, SharedSecretStore};
use axomail_storage::{
    NewMailbox, NewMessage, RetentionAction, RetentionMode, Store, raw_message_path, read_raw,
    write_raw_atomically,
};
use axomail_sync::{EngineOptions, SyncConfig, SyncEngine};

use common::{secret_stores, valid_input};

const DAY: i64 = 86_400;

/// The `apply_folder_retention` command anchors its age window on the real wall
/// clock, so date fixtures must be relative to *now* (not a fixed constant) for
/// the `by_age` window to bite as intended.
fn now_secs() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

/// Builds an offline engine so any enqueued write-back stays pending (never
/// claims), keeping the local-effect assertions race-free.
fn offline_engine(store: &Store, secrets: &SharedSecretStore) -> Arc<AppSyncEngine> {
    let factory = AppProviderFactory::new(secrets.clone(), store.clone());
    Arc::new(SyncEngine::start_with_options(
        store.clone(),
        factory,
        SyncConfig::default(),
        EngineOptions {
            offline: Arc::new(AtomicBool::new(true)),
            raw_root: None,
        },
    ))
}

/// Creates one account and returns its id, its inbox mailbox id, and a
/// freshly-seeded Trash mailbox id (IMAP accounts seed only the inbox, so the
/// Trash folder is added explicitly).
async fn account_inbox_trash(store: &Store, secrets: &SharedSecretStore) -> (i64, i64, i64) {
    let summary = create_account_impl(store, secrets, valid_input())
        .await
        .unwrap();
    let inbox = summary
        .mailboxes
        .iter()
        .find(|m| m.role == MailboxRole::Inbox)
        .expect("a seeded inbox");
    let trash = store
        .mailboxes()
        .upsert(&NewMailbox {
            account: AccountId::new(summary.id),
            name: MailboxName::new("Trash"),
            remote_name: MailboxName::new("Trash"),
            role: MailboxRole::Trash,
            uid_validity: Some(1),
            uid_next: Some(1),
        })
        .await
        .unwrap();
    (summary.id, inbox.id, trash.id.value())
}

/// Inserts a message into `mailbox` with the given date and star, returning its
/// local id.
async fn insert(
    store: &Store,
    mailbox: i64,
    uid: u32,
    date_unix: Option<i64>,
    flagged: bool,
    has_raw: bool,
) -> i64 {
    let flags = MessageFlags {
        flagged,
        ..MessageFlags::default()
    };
    let message = NewMessage {
        mailbox: MailboxId::new(mailbox),
        remote_id: Some(RemoteMessageId::Imap {
            uid,
            uid_validity: 1,
        }),
        subject: Some(format!("Message {uid}")),
        from: vec![Address::from_email(
            EmailAddress::parse("ada@example.org").unwrap(),
        )],
        to: vec![Address::from_email(
            EmailAddress::parse("klaus@theferbers.eu").unwrap(),
        )],
        cc: Vec::new(),
        bcc: Vec::new(),
        date: date_unix.map(UtcTimestamp::from_unix_seconds),
        flags,
        size_bytes: Some(2048),
        preview: Some("preview".to_owned()),
        has_raw,
    };
    store.messages().insert(&message).await.unwrap().id.value()
}

#[tokio::test]
async fn policy_round_trips_and_rejects_unknown_mailboxes() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let (_account, inbox, _trash) = account_inbox_trash(&store, &secrets).await;

    let default = get_folder_retention_impl(&store, inbox).await.unwrap();
    assert_eq!(default.mode, RetentionMode::KeepAll);

    set_folder_retention_impl(
        &store,
        inbox,
        RetentionPolicyInput {
            mode: RetentionMode::ByCount,
            days: 30,
            max_count: 500,
            action: RetentionAction::PurgeBody,
        },
    )
    .await
    .unwrap();
    let updated = get_folder_retention_impl(&store, inbox).await.unwrap();
    assert_eq!(updated.mode, RetentionMode::ByCount);
    assert_eq!(updated.max_count, 500);
    assert_eq!(updated.action, RetentionAction::PurgeBody);

    assert!(get_folder_retention_impl(&store, 9999).await.is_err());
    assert!(
        set_folder_retention_impl(
            &store,
            9999,
            RetentionPolicyInput {
                mode: RetentionMode::KeepAll,
                days: 30,
                max_count: 500,
                action: RetentionAction::Trash,
            },
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn apply_trash_moves_old_messages_and_spares_stars() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let engine = offline_engine(&store, &secrets);
    let raw = tempfile::tempdir().unwrap();
    let (_account, inbox, trash) = account_inbox_trash(&store, &secrets).await;

    let now = now_secs();
    let old_a = insert(&store, inbox, 1, Some(now - 100 * DAY), false, false).await;
    let old_b = insert(&store, inbox, 2, Some(now - 60 * DAY), false, false).await;
    let recent = insert(&store, inbox, 3, Some(now - 2 * DAY), false, false).await;
    let starred = insert(&store, inbox, 4, Some(now - 90 * DAY), true, false).await;

    set_folder_retention_impl(
        &store,
        inbox,
        RetentionPolicyInput {
            mode: RetentionMode::ByAge,
            days: 30,
            max_count: 1000,
            action: RetentionAction::Trash,
        },
    )
    .await
    .unwrap();

    let outcome = apply_folder_retention_impl(&store, &engine, raw.path(), inbox)
        .await
        .unwrap();
    assert_eq!(outcome.moved_to_trash, 2);
    assert_eq!(outcome.purged_bodies, 0);

    // The two old, unstarred messages now live in Trash; the recent and the
    // starred ones stay in the inbox.
    for (id, expected) in [
        (old_a, trash),
        (old_b, trash),
        (recent, inbox),
        (starred, inbox),
    ] {
        let envelope = store.messages().get(MessageId::new(id)).await.unwrap();
        assert_eq!(envelope.unwrap().mailbox.value(), expected);
    }
}

#[tokio::test]
async fn apply_purge_body_drops_cached_bodies_only() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let engine = offline_engine(&store, &secrets);
    let raw = tempfile::tempdir().unwrap();
    let (_account, inbox, _trash) = account_inbox_trash(&store, &secrets).await;

    // An old cached message and a recent cached message.
    let now = now_secs();
    let old = insert(&store, inbox, 1, Some(now - 100 * DAY), false, true).await;
    let recent = insert(&store, inbox, 2, Some(now - 2 * DAY), false, true).await;
    for id in [old, recent] {
        write_raw_atomically(&raw_message_path(raw.path(), id), b"cached body")
            .await
            .unwrap();
    }

    set_folder_retention_impl(
        &store,
        inbox,
        RetentionPolicyInput {
            mode: RetentionMode::ByAge,
            days: 30,
            max_count: 1000,
            action: RetentionAction::PurgeBody,
        },
    )
    .await
    .unwrap();

    let outcome = apply_folder_retention_impl(&store, &engine, raw.path(), inbox)
        .await
        .unwrap();
    assert_eq!(outcome.purged_bodies, 1);
    assert_eq!(outcome.moved_to_trash, 0);

    // The old message's body is gone (file removed, has_raw cleared); the
    // recent one keeps its body. Neither envelope was deleted.
    assert!(
        read_raw(&raw_message_path(raw.path(), old))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        read_raw(&raw_message_path(raw.path(), recent))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .messages()
            .get(MessageId::new(old))
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn keep_all_is_a_noop() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let engine = offline_engine(&store, &secrets);
    let raw = tempfile::tempdir().unwrap();
    let (_account, inbox, _trash) = account_inbox_trash(&store, &secrets).await;
    insert(
        &store,
        inbox,
        1,
        Some(now_secs() - 1000 * DAY),
        false,
        false,
    )
    .await;

    let outcome = apply_folder_retention_impl(&store, &engine, raw.path(), inbox)
        .await
        .unwrap();
    assert_eq!(outcome.moved_to_trash, 0);
    assert_eq!(outcome.purged_bodies, 0);
}
