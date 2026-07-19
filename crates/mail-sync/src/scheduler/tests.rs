//! Deterministic unit tests for the scheduler's timing and backoff logic.
//!
//! These drive the scheduler through a storage-free [`FakeSink`], so the tokio
//! virtual clock advances cleanly (no database timers perturb it) and every
//! interval assertion is exact. The test parks on a [`Notify`] between polls so
//! the paused runtime can auto-advance to the scheduler's next timer.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use axomail_core::{AccountId, MailError, MailboxId};
use axomail_storage::SyncJobId;
use tokio::sync::{Notify, broadcast};
use tokio::time::Instant;

use super::{BackgroundScheduler, JobSink, SchedulerConfig, SchedulerStatus};
use crate::event::{SyncErrorClass, SyncEvent, SyncJobSummary};
use crate::job::SyncMailbox;

const FIRST_DELAY: Duration = Duration::from_secs(5);
const INTERVAL: Duration = Duration::from_secs(60);

fn config() -> SchedulerConfig {
    SchedulerConfig {
        interval: INTERVAL,
        first_delay: FIRST_DELAY,
    }
}

fn account() -> AccountId {
    AccountId::new(1)
}

fn mailbox() -> MailboxId {
    MailboxId::new(1)
}

fn target(account: AccountId, mailbox: MailboxId) -> SyncMailbox {
    SyncMailbox {
        account_id: account,
        mailbox_id: mailbox,
    }
}

/// One observed enqueue call: when it happened and what it targeted.
#[derive(Clone, Copy)]
struct Enqueued {
    at: Instant,
    job: SyncMailbox,
    job_id: SyncJobId,
}

/// A storage-free [`JobSink`]. It records every enqueue (with a virtual-time
/// stamp and a freshly minted job id) and lets a test inject terminal events to
/// exercise offline backoff.
struct FakeSink {
    events: broadcast::Sender<SyncEvent>,
    calls: Mutex<Vec<Enqueued>>,
    signal: Notify,
    next_id: AtomicI64,
}

impl FakeSink {
    fn new() -> Arc<Self> {
        let (events, _) = broadcast::channel(256);
        Arc::new(Self {
            events,
            calls: Mutex::new(Vec::new()),
            signal: Notify::new(),
            next_id: AtomicI64::new(1),
        })
    }

    /// Number of enqueue calls observed so far.
    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    /// The nth (0-based) observed enqueue.
    fn call(&self, index: usize) -> Enqueued {
        self.calls.lock().unwrap()[index]
    }

    /// Parks until at least `count` enqueue calls have been observed, letting
    /// the paused runtime auto-advance to the scheduler's next timer.
    async fn wait_for(&self, count: usize) {
        while self.call_count() < count {
            self.signal.notified().await;
        }
    }

    /// Emits a temporary failure for a job the scheduler enqueued.
    fn fail_temporary(&self, job_id: SyncJobId) {
        let _ = self.events.send(SyncEvent::JobFailed {
            job_id,
            class: SyncErrorClass::Temporary,
            message: "scripted temporary failure".to_owned(),
        });
    }

    /// Emits a permanent failure for a job the scheduler enqueued.
    fn fail_permanent(&self, job_id: SyncJobId) {
        let _ = self.events.send(SyncEvent::JobFailed {
            job_id,
            class: SyncErrorClass::Permanent,
            message: "scripted permanent failure".to_owned(),
        });
    }

    /// Emits a success for a job the scheduler enqueued.
    fn succeed(&self, job_id: SyncJobId) {
        let _ = self.events.send(SyncEvent::JobFinished {
            job_id,
            summary: SyncJobSummary::default(),
        });
    }
}

impl JobSink for FakeSink {
    async fn enqueue(&self, job: SyncMailbox) -> Result<(), MailError> {
        let job_id = SyncJobId::new(self.next_id.fetch_add(1, Ordering::Relaxed));
        // Announce the job so the scheduler can map its id back to the account
        // when a later terminal event arrives.
        let _ = self.events.send(SyncEvent::JobQueued {
            job_id,
            account_id: job.account_id,
            mailbox_id: Some(job.mailbox_id),
        });
        self.calls.lock().unwrap().push(Enqueued {
            at: Instant::now(),
            job,
            job_id,
        });
        self.signal.notify_one();
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.events.subscribe()
    }
}

#[tokio::test(start_paused = true)]
async fn first_run_is_delayed_then_polls_at_the_interval() {
    let sink = FakeSink::new();
    let start = Instant::now();
    let scheduler = BackgroundScheduler::start_with_sink(
        Arc::clone(&sink),
        config(),
        [target(account(), mailbox())],
    );

    sink.wait_for(3).await;
    let e1 = sink.call(0);
    let e2 = sink.call(1);
    let e3 = sink.call(2);

    assert_eq!(
        e1.at - start,
        FIRST_DELAY,
        "first poll waits the first delay"
    );
    assert_eq!(
        e1.job.mailbox_id,
        mailbox(),
        "the configured mailbox is polled"
    );
    assert_eq!(e2.at - e1.at, INTERVAL, "second poll one interval later");
    assert_eq!(e3.at - e2.at, INTERVAL, "third poll one interval later");

    drop(scheduler);
}

#[tokio::test(start_paused = true)]
async fn temporary_failure_doubles_interval_and_success_resets() {
    let sink = FakeSink::new();
    let scheduler = BackgroundScheduler::start_with_sink(
        Arc::clone(&sink),
        config(),
        [target(account(), mailbox())],
    );

    sink.wait_for(1).await;
    let e1 = sink.call(0);
    sink.fail_temporary(e1.job_id); // -> interval doubles to 2x

    sink.wait_for(2).await;
    let e2 = sink.call(1);
    assert_eq!(
        e2.at - e1.at,
        INTERVAL * 2,
        "first temporary failure doubles the interval"
    );
    assert_eq!(
        backoff_secs(&scheduler, account()),
        Some(INTERVAL.as_secs() * 2),
        "status exposes the doubled interval"
    );
    sink.fail_temporary(e2.job_id); // -> interval doubles again to 4x

    sink.wait_for(3).await;
    let e3 = sink.call(2);
    assert_eq!(
        e3.at - e2.at,
        INTERVAL * 4,
        "second temporary failure doubles it again"
    );
    sink.succeed(e3.job_id); // -> reset to the base interval

    sink.wait_for(4).await;
    let e4 = sink.call(3);
    assert_eq!(
        e4.at - e3.at,
        INTERVAL,
        "a success resets to the base interval"
    );
    assert_eq!(
        backoff_secs(&scheduler, account()),
        None,
        "status clears the backoff after a success"
    );

    drop(scheduler);
}

#[tokio::test(start_paused = true)]
async fn backoff_is_capped_at_one_hour() {
    let sink = FakeSink::new();
    // A large base interval so a single doubling would exceed the cap.
    let scheduler = BackgroundScheduler::start_with_sink(
        Arc::clone(&sink),
        SchedulerConfig {
            interval: Duration::from_secs(40 * 60),
            first_delay: FIRST_DELAY,
        },
        [target(account(), mailbox())],
    );

    sink.wait_for(1).await;
    let e1 = sink.call(0);
    sink.fail_temporary(e1.job_id);

    sink.wait_for(2).await;
    assert_eq!(
        backoff_secs(&scheduler, account()),
        Some(60 * 60),
        "doubling is capped at one hour"
    );

    drop(scheduler);
}

#[tokio::test(start_paused = true)]
async fn permanent_failure_does_not_back_off() {
    let sink = FakeSink::new();
    let scheduler = BackgroundScheduler::start_with_sink(
        Arc::clone(&sink),
        config(),
        [target(account(), mailbox())],
    );

    sink.wait_for(1).await;
    let e1 = sink.call(0);
    sink.fail_permanent(e1.job_id);

    sink.wait_for(2).await;
    let e2 = sink.call(1);
    assert_eq!(
        e2.at - e1.at,
        INTERVAL,
        "a permanent failure keeps the base interval"
    );
    assert_eq!(
        backoff_secs(&scheduler, account()),
        None,
        "a permanent failure records no backoff"
    );

    drop(scheduler);
}

#[tokio::test(start_paused = true)]
async fn pause_stops_polling_and_resume_continues() {
    let sink = FakeSink::new();
    let scheduler = BackgroundScheduler::start_with_sink(
        Arc::clone(&sink),
        config(),
        [target(account(), mailbox())],
    );

    sink.wait_for(1).await;
    scheduler.pause();
    tokio::task::yield_now().await;
    assert!(!scheduler.status().running);

    // No polling happens across several intervals while paused.
    for _ in 0..5 {
        tokio::time::advance(INTERVAL).await;
        tokio::task::yield_now().await;
    }
    assert_eq!(sink.call_count(), 1, "a paused scheduler must not poll");

    scheduler.resume();
    assert!(scheduler.status().running);
    sink.wait_for(2).await; // the overdue poll fires promptly

    drop(scheduler);
}

#[tokio::test(start_paused = true)]
async fn shutdown_joins_and_stops_polling() {
    let sink = FakeSink::new();
    let scheduler = BackgroundScheduler::start_with_sink(
        Arc::clone(&sink),
        config(),
        [target(account(), mailbox())],
    );

    sink.wait_for(1).await;
    // Would hang if the task or a timer leaked.
    scheduler.shutdown().await;

    for _ in 0..3 {
        tokio::time::advance(INTERVAL).await;
        tokio::task::yield_now().await;
    }
    assert_eq!(sink.call_count(), 1, "a shut-down scheduler must not poll");
}

#[tokio::test(start_paused = true)]
async fn update_targets_redirects_the_next_poll() {
    let sink = FakeSink::new();
    let inbox = MailboxId::new(1);
    let archive = MailboxId::new(2);
    let scheduler = BackgroundScheduler::start_with_sink(
        Arc::clone(&sink),
        config(),
        [target(account(), inbox)],
    );

    sink.wait_for(1).await;
    assert_eq!(sink.call(0).job.mailbox_id, inbox, "inbox is polled first");

    scheduler.update_targets([target(account(), archive)]);
    sink.wait_for(2).await;
    assert_eq!(
        sink.call(1).job.mailbox_id,
        archive,
        "after update_targets the archive is polled"
    );

    drop(scheduler);
}

#[tokio::test(start_paused = true)]
async fn status_reports_clamped_interval() {
    let sink = FakeSink::new();
    // Sub-minute interval is clamped to the one-minute floor.
    let scheduler = BackgroundScheduler::start_with_sink(
        Arc::clone(&sink),
        SchedulerConfig {
            interval: Duration::from_secs(10),
            first_delay: FIRST_DELAY,
        },
        [target(account(), mailbox())],
    );

    let status = scheduler.status();
    assert!(status.running);
    assert_eq!(
        status.interval_secs, 60,
        "sub-minute interval clamps to the floor"
    );
    assert!(status.per_account_backoff.is_empty());

    drop(scheduler);
}

#[test]
fn status_round_trips_through_json() {
    let mut per_account_backoff = BTreeMap::new();
    per_account_backoff.insert(AccountId::new(3), 240u64);
    per_account_backoff.insert(AccountId::new(7), 3600u64);
    let status = SchedulerStatus {
        running: false,
        interval_secs: 600,
        per_account_backoff,
    };

    let json = serde_json::to_string(&status).unwrap();
    let restored: SchedulerStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, status, "account-keyed backoff map round-trips");
}

/// Reads the backoff seconds recorded for `account`, if any.
fn backoff_secs(scheduler: &BackgroundScheduler, account: AccountId) -> Option<u64> {
    scheduler
        .status()
        .per_account_backoff
        .get(&account)
        .copied()
}
