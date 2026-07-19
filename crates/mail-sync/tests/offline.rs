//! End-to-end tests for sync-time offline body download and the Work Offline
//! gate, driven by the scripted provider against an in-memory store.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use axomail_core::{Address, EmailAddress, MailboxId, MessageFlags, RemoteMessageId, UtcTimestamp};
use axomail_storage::{JobState, NewMessage, OfflineMode, Store, raw_message_path};
use axomail_sync::{EngineOptions, SyncConfig, SyncEngine, SyncMailbox};
use support::{FakeFactory, FakeWorld, drain_until_terminal, store_with_mailbox, test_config};

const UV: u32 = 100;

/// A minimal RFC 5322 message whose text body carries `body`.
fn eml(subject: &str, body: &str) -> Vec<u8> {
    format!(
        "From: ada@example.org\r\nTo: klaus@theferbers.eu\r\nSubject: {subject}\r\n\r\n{body}\r\n"
    )
    .into_bytes()
}

/// Inserts a remote (IMAP) message with no cached body and returns its id.
async fn insert_remote(store: &Store, mailbox: MailboxId, uid: u32, subject: &str) -> i64 {
    let message = NewMessage {
        mailbox,
        remote_id: Some(RemoteMessageId::Imap {
            uid,
            uid_validity: UV,
        }),
        subject: Some(subject.to_owned()),
        from: vec![Address::from_email(
            EmailAddress::parse("ada@example.org").unwrap(),
        )],
        to: vec![Address::from_email(
            EmailAddress::parse("klaus@theferbers.eu").unwrap(),
        )],
        cc: Vec::new(),
        bcc: Vec::new(),
        date: Some(UtcTimestamp::from_unix_seconds(1_700_000_000)),
        flags: MessageFlags::default(),
        size_bytes: Some(1024),
        preview: Some("preview".to_owned()),
        has_raw: false,
    };
    store.messages().insert(&message).await.unwrap().id.value()
}

/// Builds an offline-store engine over `store`/`world` with a temp raw root.
fn offline_engine(
    store: &Store,
    world: Arc<FakeWorld>,
    raw_root: &Path,
    offline: Arc<AtomicBool>,
    config: SyncConfig,
) -> SyncEngine<support::FakeProvider, FakeFactory> {
    let raw_root: Arc<Path> = raw_root.to_path_buf().into();
    SyncEngine::start_with_options(
        store.clone(),
        FakeFactory::new(world),
        config,
        EngineOptions {
            offline,
            raw_root: Some(raw_root),
        },
    )
}

#[tokio::test]
async fn sync_downloads_policy_covered_bodies_sets_has_raw_and_indexes_search() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    store
        .offline()
        .set(mailbox, OfflineMode::All, 30)
        .await
        .unwrap();

    let a = insert_remote(&store, mailbox, 1, "First").await;
    let b = insert_remote(&store, mailbox, 2, "Second").await;
    world.set_raw(
        RemoteMessageId::Imap {
            uid: 1,
            uid_validity: UV,
        },
        eml("First", "body alpha zqxwomble"),
    );
    world.set_raw(
        RemoteMessageId::Imap {
            uid: 2,
            uid_validity: UV,
        },
        eml("Second", "body beta plughxyzzy"),
    );

    let raw_dir = tempfile::tempdir().unwrap();
    let engine = offline_engine(
        &store,
        world,
        raw_dir.path(),
        Arc::new(AtomicBool::new(false)),
        test_config(),
    );
    let mut events = engine.subscribe();
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: mailbox,
        })
        .await
        .unwrap();
    let collected = drain_until_terminal(&mut events).await;

    // The job reports two cached bodies.
    let cached = collected.iter().find_map(|event| match event {
        axomail_sync::SyncEvent::JobFinished { summary, .. } => Some(summary.bodies_cached),
        _ => None,
    });
    assert_eq!(cached, Some(2));

    // Raw files landed on disk under the shared path helper.
    assert!(raw_message_path(raw_dir.path(), a).exists());
    assert!(raw_message_path(raw_dir.path(), b).exists());

    // The full body text is searchable although neither message was opened.
    let hits = store.search().search(&[], "zqxwomble", 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].message_id.value(), a);
    let other = store.search().search(&[], "plughxyzzy", 10).await.unwrap();
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].message_id.value(), b);
}

#[tokio::test]
async fn large_backlog_downloads_are_bounded_per_pass_and_progress_incrementally() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    store
        .offline()
        .set(mailbox, OfflineMode::All, 30)
        .await
        .unwrap();

    for uid in 1..=3 {
        insert_remote(&store, mailbox, uid, &format!("Msg {uid}")).await;
        world.set_raw(
            RemoteMessageId::Imap {
                uid,
                uid_validity: UV,
            },
            eml(&format!("Msg {uid}"), &format!("body number {uid}")),
        );
    }

    let raw_dir = tempfile::tempdir().unwrap();
    let config = SyncConfig {
        offline_download_batch: 2,
        ..test_config()
    };
    let engine = offline_engine(
        &store,
        Arc::clone(&world),
        raw_dir.path(),
        Arc::new(AtomicBool::new(false)),
        config,
    );

    // First pass caches at most the batch limit (2 of 3).
    let first = run_one(&engine, account, mailbox).await;
    assert_eq!(first, 2);

    // A second job over the same mailbox drains the remaining one.
    let second = run_one(&engine, account, mailbox).await;
    assert_eq!(second, 1);

    // A third finds nothing left.
    let third = run_one(&engine, account, mailbox).await;
    assert_eq!(third, 0);
    assert_eq!(world.fetch_calls(), 3);
}

/// Enqueues one mailbox sync, waits for it to finish, and returns the number
/// of bodies cached by that job.
async fn run_one(
    engine: &SyncEngine<support::FakeProvider, FakeFactory>,
    account: axomail_core::AccountId,
    mailbox: MailboxId,
) -> u64 {
    let mut events = engine.subscribe();
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: mailbox,
        })
        .await
        .unwrap();
    let collected = drain_until_terminal(&mut events).await;
    collected
        .iter()
        .find_map(|event| match event {
            axomail_sync::SyncEvent::JobFinished { summary, .. } => Some(summary.bodies_cached),
            _ => None,
        })
        .expect("a JobFinished event")
}

#[tokio::test]
async fn offline_engine_claims_no_jobs_then_resumes_when_back_online() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    world.set_plan(
        mailbox,
        vec![support::Pass::Batch(support::batch(
            UV,
            2,
            false,
            vec![
                support::envelope(1, UV, "first", false),
                support::envelope(2, UV, "second", false),
            ],
        ))],
    );

    let offline = Arc::new(AtomicBool::new(true));
    let raw_dir = tempfile::tempdir().unwrap();
    let engine = offline_engine(
        &store,
        Arc::clone(&world),
        raw_dir.path(),
        Arc::clone(&offline),
        test_config(),
    );
    let mut events = engine.subscribe();
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: mailbox,
        })
        .await
        .unwrap();

    // While offline the worker never claims: no sync pass runs and the job
    // stays pending.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(world.sync_calls(mailbox), 0);
    let pending = store
        .sync_jobs()
        .list_by_state(JobState::Pending)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);

    // Going back online resumes the pending job, which now runs to completion.
    engine.set_offline(false);
    let collected = drain_until_terminal(&mut events).await;
    assert!(
        collected
            .iter()
            .any(|event| matches!(event, axomail_sync::SyncEvent::JobFinished { .. }))
    );
    assert!(world.sync_calls(mailbox) >= 1);
    assert_eq!(support::message_count(&store, mailbox).await, 2);
}
