//! Application-side control of the background sync scheduler.
//!
//! [`axomail_sync::BackgroundScheduler`] fixes its polling interval at
//! construction and exposes no setter, so changing the interval means
//! replacing the scheduler. [`SchedulerControl`] owns the current instance
//! behind a lock and rebuilds it when the interval changes, while cheaper
//! changes (an account added or removed) reuse the running scheduler through
//! [`BackgroundScheduler::update_targets`].
//!
//! Targets are always "every account's inbox-role mailbox": the scheduler
//! keeps inboxes fresh; other folders sync on demand.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axomail_core::{MailError, MailboxRole};
use axomail_storage::Store;
use axomail_sync::{BackgroundScheduler, SchedulerConfig, SchedulerStatus, SyncMailbox};

use crate::state::AppSyncEngine;

/// Delay before the first background poll, keeping application start snappy.
const FIRST_DELAY: Duration = Duration::from_secs(30);

/// Seconds per minute, for interval conversion.
const SECS_PER_MINUTE: u64 = 60;

/// Owns and manages the process's single [`BackgroundScheduler`].
pub struct SchedulerControl {
    engine: Arc<AppSyncEngine>,
    store: Store,
    inner: Mutex<Option<BackgroundScheduler>>,
    /// Global Work Offline flag (shared with [`AppState`](crate::state::AppState)
    /// and the engine). The scheduler is paused while it is set so no polls are
    /// enqueued offline; the engine gate is the ultimate guarantee regardless.
    offline: Arc<AtomicBool>,
}

impl SchedulerControl {
    /// Creates a control that will drive `engine`, reading targets from
    /// `store`. No scheduler runs until [`launch`](Self::launch) is called.
    #[must_use]
    pub fn new(engine: Arc<AppSyncEngine>, store: Store, offline: Arc<AtomicBool>) -> Self {
        Self {
            engine,
            store,
            inner: Mutex::new(None),
            offline,
        }
    }

    /// Starts (or restarts) the scheduler at `interval_minutes`, targeting
    /// every account's inbox. Any previous scheduler is stopped. If the app is
    /// currently offline the fresh scheduler starts paused, so relaunching for
    /// an interval change never resumes polling behind the offline toggle.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] if the target set cannot be read.
    pub async fn launch(&self, interval_minutes: u32) -> Result<(), MailError> {
        let targets = inbox_targets(&self.store).await?;
        let config = SchedulerConfig {
            interval: Duration::from_secs(u64::from(interval_minutes) * SECS_PER_MINUTE),
            first_delay: FIRST_DELAY,
        };
        let scheduler = BackgroundScheduler::start(Arc::clone(&self.engine), config, targets);
        if self.offline.load(Ordering::SeqCst) {
            scheduler.pause();
        }
        // Replacing drops the previous handle, whose `Drop` stops its task.
        *lock(&self.inner) = Some(scheduler);
        Ok(())
    }

    /// Pauses or resumes background polling to match the Work Offline toggle.
    /// A no-op when no scheduler has been launched yet.
    pub fn set_offline(&self, offline: bool) {
        if let Some(scheduler) = lock(&self.inner).as_ref() {
            if offline {
                scheduler.pause();
            } else {
                scheduler.resume();
            }
        }
    }

    /// Recomputes the target set and pushes it into the running scheduler.
    /// A no-op when no scheduler has been launched yet.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] if the target set cannot be read.
    pub async fn refresh_targets(&self) -> Result<(), MailError> {
        let targets = inbox_targets(&self.store).await?;
        if let Some(scheduler) = lock(&self.inner).as_ref() {
            scheduler.update_targets(targets);
        }
        Ok(())
    }

    /// Returns the running scheduler's status snapshot, or `None` if it has
    /// not been launched.
    #[must_use]
    pub fn status(&self) -> Option<SchedulerStatus> {
        lock(&self.inner).as_ref().map(BackgroundScheduler::status)
    }
}

/// Builds the scheduler's target set: one entry per account inbox mailbox.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
async fn inbox_targets(store: &Store) -> Result<Vec<SyncMailbox>, MailError> {
    let mut targets = Vec::new();
    for account in store.accounts().list().await? {
        for mailbox in store.mailboxes().list(account.id).await? {
            if mailbox.role == MailboxRole::Inbox {
                targets.push(SyncMailbox {
                    account_id: account.id,
                    mailbox_id: mailbox.id,
                });
            }
        }
    }
    Ok(targets)
}

/// Locks a [`Mutex`], recovering the guard even if a previous holder
/// panicked; the guarded value is a plain handle, so recovery is safe.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
