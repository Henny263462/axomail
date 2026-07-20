//! The scheduler's single background task.
//!
//! One `tokio::select` loop drives everything: it recomputes the next per-account
//! deadline, then races a shutdown token, a wake signal (state changed via the
//! public API), the engine event stream (for offline backoff), and one timer.
//! Only one timer is ever armed, so there is no busy-waiting, and cancellation
//! is observed promptly at every await point.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axomail_core::{AccountId, MailboxId};
use axomail_storage::SyncJobId;
use tokio::sync::Notify;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::event::{SyncErrorClass, SyncEvent};
use crate::job::SyncMailbox;

use super::{JobSink, MAX_BACKOFF, SchedulerConfig, SchedulerState, lock};

/// Runs the scheduler loop until the shutdown token is cancelled.
pub(super) async fn run<S: JobSink>(
    sink: Arc<S>,
    state: Arc<Mutex<SchedulerState>>,
    wake: Arc<Notify>,
    shutdown: CancellationToken,
    config: SchedulerConfig,
) {
    let base = config.base_interval();
    let first_delay = config.first_delay;
    let mut events = sink.subscribe();
    let mut events_open = true;
    // Next fire time per targeted account.
    let mut schedule: HashMap<AccountId, Instant> = HashMap::new();
    // Maps a queued job back to its account so terminal events adjust the
    // right account's backoff.
    let mut jobs: HashMap<SyncJobId, AccountId> = HashMap::new();

    loop {
        let now = Instant::now();
        let (paused, accounts) = snapshot(&state);
        reconcile(&mut schedule, &accounts, now, first_delay);

        // Arm a single timer for the earliest due account; none while paused.
        let deadline = if paused {
            None
        } else {
            schedule.values().copied().min()
        };
        let armed = deadline.is_some();
        let wake_at = deadline.unwrap_or(now);

        tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            () = wake.notified() => {}
            result = events.recv(), if events_open => match result {
                Ok(event) => handle_event(&event, &state, &mut schedule, &mut jobs, base),
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => events_open = false,
            },
            () = tokio::time::sleep_until(wake_at), if armed => {
                fire_due(&sink, &state, &mut schedule, &shutdown, base).await;
            }
        }
    }
}

/// Reads the paused flag and the current set of targeted accounts.
fn snapshot(state: &Mutex<SchedulerState>) -> (bool, BTreeSet<AccountId>) {
    let state = lock(state);
    (state.paused, state.targets.keys().copied().collect())
}

/// Drops schedule entries for accounts no longer targeted and gives newly
/// targeted accounts a first-run delay before their initial poll.
fn reconcile(
    schedule: &mut HashMap<AccountId, Instant>,
    accounts: &BTreeSet<AccountId>,
    now: Instant,
    first_delay: Duration,
) {
    schedule.retain(|account, _| accounts.contains(account));
    for account in accounts {
        schedule.entry(*account).or_insert(now + first_delay);
    }
}

/// Enqueues every mailbox of every account whose deadline has arrived, then
/// reschedules that account by its current (possibly backed-off) interval.
async fn fire_due<S: JobSink>(
    sink: &Arc<S>,
    state: &Mutex<SchedulerState>,
    schedule: &mut HashMap<AccountId, Instant>,
    shutdown: &CancellationToken,
    base: Duration,
) {
    let now = Instant::now();
    let due: Vec<AccountId> = schedule
        .iter()
        .filter_map(|(account, at)| (*at <= now).then_some(*account))
        .collect();
    if due.is_empty() {
        return;
    }

    // Snapshot the plan under the lock, then release it before awaiting.
    let plan: Vec<(AccountId, Vec<MailboxId>, Duration)> = {
        let state = lock(state);
        if state.paused {
            return;
        }
        due.iter()
            .filter_map(|account| {
                let mailboxes = state.targets.get(account)?.iter().copied().collect();
                let interval = state.backoff.get(account).copied().unwrap_or(base);
                Some((*account, mailboxes, interval))
            })
            .collect()
    };

    for (account, mailboxes, interval) in plan {
        schedule.insert(account, now + interval);
        for mailbox_id in mailboxes {
            let job = SyncMailbox {
                account_id: account,
                mailbox_id,
            };
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return,
                result = sink.enqueue(job) => {
                    if let Err(error) = result {
                        tracing::debug!(error = %error, account_id = %account, "scheduler enqueue rejected");
                    }
                }
            }
        }

        // Periodic write-back backstop: drain any queued offline ops for this
        // account. De-duplicated by the engine, so a running replay is not
        // piled on; a no-op when the queue is empty.
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            result = sink.enqueue_op_replay(account) => {
                if let Err(error) = result {
                    tracing::debug!(error = %error, account_id = %account, "scheduler op-replay enqueue rejected");
                }
            }
        }
    }
}

/// Updates the job/account map and per-account backoff from an engine event.
fn handle_event(
    event: &SyncEvent,
    state: &Mutex<SchedulerState>,
    schedule: &mut HashMap<AccountId, Instant>,
    jobs: &mut HashMap<SyncJobId, AccountId>,
    base: Duration,
) {
    match event {
        SyncEvent::JobQueued {
            job_id, account_id, ..
        } => {
            jobs.insert(*job_id, *account_id);
        }
        SyncEvent::JobFinished { job_id, .. } => {
            if let Some(account) = jobs.remove(job_id) {
                on_success(state, schedule, account, base);
            }
        }
        SyncEvent::JobFailed { job_id, class, .. } => {
            if let Some(account) = jobs.remove(job_id)
                && *class == SyncErrorClass::Temporary
            {
                on_temporary_failure(state, schedule, account, base);
            }
        }
        SyncEvent::JobCancelled { job_id } => {
            // A cancellation is neither success nor failure: forget the job
            // without touching the account's backoff schedule.
            jobs.remove(job_id);
        }
        SyncEvent::JobStarted { .. }
        | SyncEvent::BatchIngested { .. }
        | SyncEvent::OpsReplayed { .. }
        // Push-connection status is informational; it never drives poll backoff.
        | SyncEvent::PushStateChanged { .. } => {}
    }
}

/// Doubles an account's interval (capped) after a temporary failure and pushes
/// its next poll out accordingly.
fn on_temporary_failure(
    state: &Mutex<SchedulerState>,
    schedule: &mut HashMap<AccountId, Instant>,
    account: AccountId,
    base: Duration,
) {
    let interval = {
        let mut state = lock(state);
        if !state.targets.contains_key(&account) {
            state.backoff.remove(&account);
            return;
        }
        let current = state.backoff.get(&account).copied().unwrap_or(base);
        let doubled = current
            .checked_mul(2)
            .unwrap_or(MAX_BACKOFF)
            .min(MAX_BACKOFF);
        state.backoff.insert(account, doubled);
        doubled
    };
    schedule.insert(account, Instant::now() + interval);
}

/// Clears an account's backoff after a success and returns it to the base
/// cadence. A success while already at the base interval is left untouched so
/// the periodic rhythm is preserved.
fn on_success(
    state: &Mutex<SchedulerState>,
    schedule: &mut HashMap<AccountId, Instant>,
    account: AccountId,
    base: Duration,
) {
    let reschedule = {
        let mut state = lock(state);
        let was_backed_off = state.backoff.remove(&account).is_some();
        was_backed_off && state.targets.contains_key(&account)
    };
    if reschedule {
        schedule.insert(account, Instant::now() + base);
    }
}
