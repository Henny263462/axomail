//! End-to-end scheduler tests against the real [`SyncEngine`], driven by the
//! scripted [`support::FakeProvider`].
//!
//! Fine-grained timing and backoff are covered by the crate's deterministic
//! unit tests (which use a storage-free sink so the virtual clock is exact).
//! These integration tests instead prove the scheduler drives the real engine:
//! a scheduled poll performs an actual sync, and repeated polls against a busy
//! engine are de-duplicated rather than piling up. They run in real time with a
//! short first delay, since the in-memory SQLite pool cannot be driven under a
//! paused clock.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use axomail_core::{AccountId, MailboxId};
use axomail_sync::{
    BackgroundScheduler, SchedulerConfig, SyncConfig, SyncEngine, SyncEvent, SyncMailbox,
};
use support::{
    FakeFactory, FakeWorld, Pass, batch, envelope, message_count, store_with_mailbox, test_config,
};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};

const UV: u32 = 100;
const INTERVAL: Duration = Duration::from_secs(60);
const FIRST_DELAY: Duration = Duration::from_millis(20);

fn scheduler_config() -> SchedulerConfig {
    SchedulerConfig {
        interval: INTERVAL,
        first_delay: FIRST_DELAY,
    }
}

fn target(account: AccountId, mailbox: MailboxId) -> SyncMailbox {
    SyncMailbox {
        account_id: account,
        mailbox_id: mailbox,
    }
}

#[tokio::test]
async fn scheduler_drives_a_real_sync() {
    let (store, account, mailbox) = store_with_mailbox().await;
    let world = FakeWorld::new();
    world.set_plan(
        mailbox,
        vec![Pass::Batch(batch(
            UV,
            2,
            false,
            vec![
                envelope(1, UV, "first", false),
                envelope(2, UV, "second", false),
            ],
        ))],
    );
    let engine = Arc::new(SyncEngine::start(
        store.clone(),
        FakeFactory::new(world),
        test_config(),
    ));
    let mut events = engine.subscribe();
    let scheduler = BackgroundScheduler::start(
        Arc::clone(&engine),
        scheduler_config(),
        [target(account, mailbox)],
    );

    // The scheduled poll runs an actual sync and ingests the mailbox.
    wait_for_finished(&mut events).await;
    assert_eq!(
        message_count(&store, mailbox).await,
        2,
        "the scheduled poll ingested the mailbox"
    );

    drop(scheduler);
}

#[tokio::test]
async fn busy_engine_polls_are_deduplicated() {
    let (store, account, mailbox) = store_with_mailbox().await;
    let world = FakeWorld::new();
    // The one job parks inside its sync pass and stays there, keeping the engine
    // busy across polls. A very large network timeout keeps that parked pass
    // alive while virtual time is advanced past many polling intervals (the
    // default 30s deadline would otherwise time the pass out and free the slot).
    world.arm_pause(None);
    let config = SyncConfig {
        network_timeout: Duration::from_secs(24 * 60 * 60),
        ..test_config()
    };
    let engine = Arc::new(SyncEngine::start(
        store,
        FakeFactory::new(world.clone()),
        config,
    ));
    // Counts every job ever announced for the whole test.
    let mut all_events = engine.subscribe();
    let mut events = engine.subscribe();
    let scheduler = BackgroundScheduler::start(
        Arc::clone(&engine),
        scheduler_config(),
        [target(account, mailbox)],
    );

    // Wait (real time) until the first poll's job is parked *inside* its sync
    // pass, past all its early database work, so advancing the clock cannot time
    // those queries out and fail the job.
    wait_for_started(&mut events).await;
    while world.max_active(account) == 0 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // Fast-forward through several polling intervals while the engine is busy.
    // Assertions read only the broadcast stream (never the pool), so the paused
    // clock cannot perturb them.
    tokio::time::pause();
    for _ in 0..5 {
        tokio::time::advance(INTERVAL).await;
        tokio::task::yield_now().await;
    }

    // Despite the first poll plus five re-polls, exactly one job was ever
    // queued: the busy engine de-duplicated every repeat.
    assert_eq!(
        count_queued(&collect_available(&mut all_events)),
        1,
        "repeated polls against a busy engine collapse to a single job"
    );

    engine.cancel_all();
    tokio::time::resume();
    drop(scheduler);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn wait_for_finished(events: &mut Receiver<SyncEvent>) {
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv()).await {
            Ok(Ok(SyncEvent::JobFinished { .. })) => return,
            Ok(Ok(_)) | Ok(Err(RecvError::Lagged(_))) => {}
            Ok(Err(RecvError::Closed)) | Err(_) => panic!("job never finished"),
        }
    }
}

async fn wait_for_started(events: &mut Receiver<SyncEvent>) {
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv()).await {
            Ok(Ok(SyncEvent::JobStarted { .. })) => return,
            Ok(Ok(_)) | Ok(Err(RecvError::Lagged(_))) => {}
            Ok(Err(RecvError::Closed)) | Err(_) => panic!("job never started"),
        }
    }
}

fn collect_available(events: &mut Receiver<SyncEvent>) -> Vec<SyncEvent> {
    let mut seen = Vec::new();
    loop {
        match events.try_recv() {
            Ok(event) => seen.push(event),
            Err(TryRecvError::Lagged(_)) => {}
            Err(TryRecvError::Empty | TryRecvError::Closed) => return seen,
        }
    }
}

fn count_queued(events: &[SyncEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, SyncEvent::JobQueued { .. }))
        .count()
}
