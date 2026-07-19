//! Integration tests for the offline-store commands: the Work Offline toggle,
//! per-mailbox policy, download-now, and cache cleanup.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use axomail_core::{
    Address, EmailAddress, MailboxId, MailboxRole, MessageFlags, RemoteMessageId, UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::compose::{list_outbox_impl, send_message_impl};
use axomail_desktop_lib::commands::offline::{
    cleanup_offline_cache_impl, download_mailbox_now_impl, get_mailbox_offline_policy_impl,
    set_mailbox_offline_policy_impl, set_work_offline_impl,
};
use axomail_desktop_lib::composition::AppProviderFactory;
use axomail_desktop_lib::scheduler::SchedulerControl;
use axomail_desktop_lib::settings::work_offline;
use axomail_desktop_lib::state::{AppSyncEngine, SharedSecretStore};
use axomail_storage::{NewMessage, OfflineMode, Store, raw_message_path, write_raw_atomically};
use axomail_sync::{EngineOptions, SyncConfig, SyncEngine, SyncEvent};

use common::{compose_input, secret_stores, valid_input};

/// Builds an engine and scheduler control sharing one offline flag.
fn engine_and_scheduler(
    store: &Store,
    secrets: &SharedSecretStore,
    offline: Arc<AtomicBool>,
) -> (Arc<AppSyncEngine>, SchedulerControl) {
    let factory = AppProviderFactory::new(secrets.clone(), store.clone());
    let engine = Arc::new(SyncEngine::start_with_options(
        store.clone(),
        factory,
        SyncConfig::default(),
        EngineOptions {
            offline: Arc::clone(&offline),
            raw_root: None,
        },
    ));
    let scheduler = SchedulerControl::new(Arc::clone(&engine), store.clone(), offline);
    (engine, scheduler)
}

/// Creates one account and returns its id and its inbox mailbox id.
async fn account_with_inbox(store: &Store, secrets: &SharedSecretStore) -> (i64, i64) {
    let summary = create_account_impl(store, secrets, valid_input())
        .await
        .unwrap();
    let inbox = summary
        .mailboxes
        .iter()
        .find(|m| m.role == MailboxRole::Inbox)
        .expect("a seeded inbox");
    (summary.id, inbox.id)
}

#[tokio::test]
async fn set_work_offline_toggles_the_engine_and_persists_the_setting() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let offline = Arc::new(AtomicBool::new(false));
    let (engine, scheduler) = engine_and_scheduler(&store, &secrets, Arc::clone(&offline));

    set_work_offline_impl(&store, &engine, &scheduler, true)
        .await
        .unwrap();
    assert!(engine.is_offline());
    assert!(work_offline(&store).await.unwrap());

    set_work_offline_impl(&store, &engine, &scheduler, false)
        .await
        .unwrap();
    assert!(!engine.is_offline());
    assert!(!work_offline(&store).await.unwrap());
}

#[tokio::test]
async fn mailbox_offline_policy_round_trips_and_rejects_unknown_mailboxes() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let (_account, inbox) = account_with_inbox(&store, &secrets).await;

    // Default policy.
    let policy = get_mailbox_offline_policy_impl(&store, inbox)
        .await
        .unwrap();
    assert_eq!(policy.mode, OfflineMode::Recent);
    assert_eq!(policy.recent_days, 30);

    set_mailbox_offline_policy_impl(&store, inbox, OfflineMode::All, 7)
        .await
        .unwrap();
    let updated = get_mailbox_offline_policy_impl(&store, inbox)
        .await
        .unwrap();
    assert_eq!(updated.mode, OfflineMode::All);
    assert_eq!(updated.recent_days, 7);

    // Unknown mailbox is rejected on both read and write.
    assert!(get_mailbox_offline_policy_impl(&store, 9999).await.is_err());
    assert!(
        set_mailbox_offline_policy_impl(&store, 9999, OfflineMode::None, 5)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn download_now_marks_full_download_and_enqueues_a_job() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    // Offline engine: the enqueued job stays pending (never claims), so the
    // assertions are race-free.
    let offline = Arc::new(AtomicBool::new(true));
    let (engine, _scheduler) = engine_and_scheduler(&store, &secrets, Arc::clone(&offline));
    let (_account, inbox) = account_with_inbox(&store, &secrets).await;

    let mut events = engine.subscribe();
    download_mailbox_now_impl(&store, &engine, inbox)
        .await
        .unwrap();

    // The mailbox is marked for a forced full download.
    let plan = store
        .offline()
        .download_plan(MailboxId::new(inbox), 0, 10)
        .await
        .unwrap();
    assert!(plan.force_full);

    // A job was enqueued for the mailbox (announced before it is claimed).
    let queued = wait_for_queued(&mut events, inbox).await;
    assert!(queued, "expected a JobQueued event for the download");
}

/// Waits (bounded) for a `JobQueued` event naming `mailbox`.
async fn wait_for_queued(
    events: &mut tokio::sync::broadcast::Receiver<SyncEvent>,
    mailbox: i64,
) -> bool {
    let deadline = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match events.recv().await {
                Ok(SyncEvent::JobQueued {
                    mailbox_id: Some(id),
                    ..
                }) if id.value() == mailbox => return true,
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(_) => return false,
            }
        }
    });
    deadline.await.unwrap_or(false)
}

#[tokio::test]
async fn offline_send_queues_in_the_outbox_without_submitting() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let (account, _inbox) = account_with_inbox(&store, &secrets).await;

    // No SMTP server is reachable; the offline gate must skip submission
    // entirely, so this cannot fail with a network error.
    let input = compose_input(account, &["dest@example.org"], "While offline");
    let outcome = send_message_impl(&store, &secrets, &[], input, true)
        .await
        .unwrap();
    assert!(outcome.queued_offline);
    assert!(!outcome.sent);
    assert!(outcome.error.is_none());

    // The message waits in the outbox as a pending (queued) row.
    let items = list_outbox_impl(&store).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].state, "pending");
    assert_eq!(items[0].attempts, 0);
}

#[tokio::test]
async fn cleanup_offline_cache_removes_stale_files_and_clears_has_raw() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let (_account, inbox) = account_with_inbox(&store, &secrets).await;
    let raw_dir = tempfile::tempdir().unwrap();

    // A cached remote message with an old date.
    let message = NewMessage {
        mailbox: MailboxId::new(inbox),
        remote_id: Some(RemoteMessageId::Imap {
            uid: 1,
            uid_validity: 1,
        }),
        subject: Some("Old".to_owned()),
        from: vec![Address::from_email(
            EmailAddress::parse("ada@example.org").unwrap(),
        )],
        to: vec![Address::from_email(
            EmailAddress::parse("klaus@theferbers.eu").unwrap(),
        )],
        cc: Vec::new(),
        bcc: Vec::new(),
        date: Some(UtcTimestamp::from_unix_seconds(1_000_000_000)),
        flags: MessageFlags::default(),
        size_bytes: Some(64),
        preview: Some("preview".to_owned()),
        has_raw: true,
    };
    let id = store.messages().insert(&message).await.unwrap().id.value();
    let path = raw_message_path(raw_dir.path(), id);
    write_raw_atomically(&path, b"cached body").await.unwrap();
    assert!(path.exists());

    // With a 'none' policy every cached body is purgeable.
    set_mailbox_offline_policy_impl(&store, inbox, OfflineMode::None, 30)
        .await
        .unwrap();
    let purged = cleanup_offline_cache_impl(&store, raw_dir.path(), inbox)
        .await
        .unwrap();
    assert_eq!(purged, 1);
    assert!(!path.exists());

    // has_raw was cleared, so under 'all' the message is a download candidate.
    set_mailbox_offline_policy_impl(&store, inbox, OfflineMode::All, 30)
        .await
        .unwrap();
    let plan = store
        .offline()
        .download_plan(MailboxId::new(inbox), 2_000_000_000, 10)
        .await
        .unwrap();
    assert_eq!(plan.candidates.len(), 1);
    assert_eq!(plan.candidates[0].message_id.value(), id);
}
