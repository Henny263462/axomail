//! IMAP IDLE push supervision.
//!
//! [`PushSupervisor`] keeps one persistent IDLE connection open per target
//! mailbox so new mail triggers an **immediate** sync instead of waiting for the
//! next poll. It runs alongside the [`SyncEngine`] and the
//! [`BackgroundScheduler`](crate::BackgroundScheduler): push is an accelerator,
//! not a replacement, so the poller keeps running as a backstop and covers any
//! account whose server does not advertise IDLE.
//!
//! # Connection model
//!
//! Each push task opens its **own** connection, distinct from the engine's
//! sync/write-back connection and held **outside** the engine's per-account job
//! lock. That connection only ever `SELECT`s a mailbox and `IDLE`s — it never
//! mutates state — so it cannot contend with the write-back connection for the
//! same session. When the server pushes a change the supervisor enqueues a
//! normal [`SyncMailbox`](crate::SyncMailbox) job, which runs on the engine's
//! connection under the usual lock.
//!
//! # Lifecycle
//!
//! The supervisor is enabled/disabled as a whole (the app gates it on the user
//! setting and the Work Offline flag) and its target set is updated as accounts
//! come and go. Each per-account task ([`task`]) handles renewal (re-issuing
//! IDLE before the server's ~29-minute inactivity limit), a clean `DONE` on
//! cancel, and reconnect with backoff on drop. Dropping the handle (or calling
//! [`PushSupervisor::shutdown`]) stops every task.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axomail_core::{AccountId, IncomingMailProvider, MailboxId, MailboxName};
use axomail_storage::Store;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::engine::SyncEngine;
use crate::factory::ProviderFactory;

mod task;

/// A mailbox to hold a push (IDLE) connection open on.
///
/// One target per account: IDLE watches a single mailbox (the inbox), which is
/// where new mail lands; other folders sync on demand or via the poller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushTarget {
    /// Account that owns the mailbox.
    pub account_id: AccountId,
    /// Local id of the mailbox to sync when the server pushes a change.
    pub mailbox_id: MailboxId,
    /// Decoded remote name to `SELECT` for IDLE.
    pub remote_name: MailboxName,
}

/// Tuning for a [`PushSupervisor`].
///
/// [`PushConfig::default`] is production-sensible; tests shorten the timers.
#[derive(Debug, Clone, Copy)]
pub struct PushConfig {
    /// How long one IDLE waits before it is re-issued (renewal). Kept under the
    /// server's ~29-minute inactivity limit; RFC 2177 advises renewing at least
    /// that often.
    pub renew_after: Duration,
    /// Base delay for the first reconnect backoff step after a drop.
    pub reconnect_base: Duration,
    /// Upper bound on any single reconnect backoff delay.
    pub reconnect_cap: Duration,
}

impl Default for PushConfig {
    fn default() -> Self {
        Self {
            // A safe margin under the 29-minute RFC 2177 ceiling.
            renew_after: Duration::from_secs(24 * 60),
            reconnect_base: Duration::from_secs(2),
            reconnect_cap: Duration::from_secs(300),
        }
    }
}

/// A running per-account push task and the target it serves.
struct Child {
    /// The target being watched; used to detect a changed mailbox on update.
    target: PushTarget,
    /// Cancels just this account's task. Every await in the task is
    /// cancellation-aware, so a graceful stop cancels and then **detaches** the
    /// handle: the task promptly sends a clean `DONE`, disconnects, and exits on
    /// its own. Only [`PushSupervisor`]'s `Drop` aborts, as a teardown safety net.
    token: CancellationToken,
    /// The spawned task handle; aborted only on `Drop` (see [`Self::token`]).
    handle: JoinHandle<()>,
}

/// Cancels a child's token and detaches its task, so it exits gracefully after
/// sending a clean `DONE` (no abort — the task is fully cancellation-aware).
fn stop_gracefully(child: Child) {
    child.token.cancel();
    // `child.handle` drops here: dropping a `JoinHandle` detaches the task,
    // which the cancellation drives to a prompt, clean exit.
}

/// State shared behind the handle's lock.
struct Inner {
    /// Whether push is currently enabled at all.
    enabled: bool,
    /// Desired targets, keyed by account.
    targets: BTreeMap<AccountId, PushTarget>,
    /// Live tasks, keyed by account.
    children: BTreeMap<AccountId, Child>,
}

/// Supervises IMAP IDLE push connections for a set of accounts.
///
/// Generic over the same provider `P` and factory `F` as the [`SyncEngine`] it
/// drives; the composition root instantiates it once with the app's concrete
/// types.
pub struct PushSupervisor<P, F>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    engine: Arc<SyncEngine<P, F>>,
    factory: Arc<F>,
    store: Store,
    config: PushConfig,
    inner: Mutex<Inner>,
}

impl<P, F> PushSupervisor<P, F>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    /// Starts a supervisor driving `engine`, watching `targets` when `enabled`.
    ///
    /// The provider factory is reused from the engine, so the composition root
    /// supplies it only once. When `enabled` is false (push turned off, or the
    /// app is offline) no task starts until [`set_enabled(true)`](Self::set_enabled).
    #[must_use]
    pub fn start(
        engine: Arc<SyncEngine<P, F>>,
        store: Store,
        config: PushConfig,
        enabled: bool,
        targets: impl IntoIterator<Item = PushTarget>,
    ) -> Self {
        let factory = engine.provider_factory();
        let targets = targets
            .into_iter()
            .map(|target| (target.account_id, target))
            .collect();
        let supervisor = Self {
            engine,
            factory,
            store,
            config,
            inner: Mutex::new(Inner {
                enabled,
                targets,
                children: BTreeMap::new(),
            }),
        };
        supervisor.reconcile();
        supervisor
    }

    /// Enables or disables push as a whole.
    ///
    /// Disabling cancels every task (each sending a clean `DONE`); enabling
    /// starts one per target. Idempotent.
    pub fn set_enabled(&self, enabled: bool) {
        {
            let mut inner = lock(&self.inner);
            if inner.enabled == enabled {
                return;
            }
            inner.enabled = enabled;
        }
        self.reconcile();
    }

    /// Replaces the set of watched targets (e.g. after accounts change).
    ///
    /// Tasks for dropped accounts are cancelled; new accounts get a task (when
    /// enabled); an account whose target mailbox changed is restarted.
    pub fn update_targets(&self, targets: impl IntoIterator<Item = PushTarget>) {
        {
            let mut inner = lock(&self.inner);
            inner.targets = targets
                .into_iter()
                .map(|target| (target.account_id, target))
                .collect();
        }
        self.reconcile();
    }

    /// Whether push is currently enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        lock(&self.inner).enabled
    }

    /// The accounts that currently have a live push task (for tests/status).
    #[must_use]
    pub fn active_accounts(&self) -> Vec<AccountId> {
        lock(&self.inner).children.keys().copied().collect()
    }

    /// Stops every push task and clears the supervisor. After this returns no
    /// task is left running.
    pub fn shutdown(&self) {
        let mut inner = lock(&self.inner);
        inner.enabled = false;
        for (_, child) in std::mem::take(&mut inner.children) {
            stop_gracefully(child);
        }
    }

    /// Brings the live task set in line with `enabled` + `targets`: cancels
    /// tasks that should stop, spawns tasks that should start, and restarts a
    /// task whose target mailbox changed.
    ///
    /// A restart (an account still wanted but with a changed target) hands the
    /// outgoing task's handle to its replacement as a `predecessor`, so the new
    /// task awaits the old connection's clean teardown before opening its own —
    /// the two IDLE sessions never overlap.
    fn reconcile(&self) {
        let mut inner = lock(&self.inner);
        // Effective targets: none while disabled.
        let desired: BTreeMap<AccountId, PushTarget> = if inner.enabled {
            inner.targets.clone()
        } else {
            BTreeMap::new()
        };

        // Stop tasks that are no longer wanted, or whose target changed. Cancel
        // each immediately; if its account will be *restarted*, keep the handle
        // so the replacement can await teardown, otherwise detach it.
        let to_stop: Vec<AccountId> = inner
            .children
            .iter()
            .filter(|(account, child)| desired.get(*account) != Some(&child.target))
            .map(|(account, _)| *account)
            .collect();
        let mut predecessors: BTreeMap<AccountId, JoinHandle<()>> = BTreeMap::new();
        for account in to_stop {
            if let Some(child) = inner.children.remove(&account) {
                child.token.cancel();
                if desired.contains_key(&account) {
                    // Restart: the replacement awaits this handle before it
                    // connects (no overlapping IDLE session for the account).
                    predecessors.insert(account, child.handle);
                }
                // Otherwise the handle drops here, detaching the cancelled task
                // to exit cleanly on its own.
            }
        }

        // Start tasks for wanted accounts that have none running.
        for (account, target) in desired {
            if inner.children.contains_key(&account) {
                continue;
            }
            let token = CancellationToken::new();
            let handle = tokio::spawn(task::run(
                Arc::clone(&self.engine),
                Arc::clone(&self.factory),
                self.store.clone(),
                target.clone(),
                self.config,
                token.clone(),
                predecessors.remove(&account),
            ));
            inner.children.insert(
                account,
                Child {
                    target,
                    token,
                    handle,
                },
            );
        }
    }
}

impl<P, F> Drop for PushSupervisor<P, F>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    fn drop(&mut self) {
        // Teardown: cancel first so a still-running task can send a clean DONE,
        // then abort as a hard safety net (unlike the graceful stop paths, Drop
        // cannot await the task, so it does not rely solely on cancellation).
        if let Ok(mut inner) = self.inner.lock() {
            for (_, child) in std::mem::take(&mut inner.children) {
                child.token.cancel();
                child.handle.abort();
            }
        }
    }
}

/// Locks the inner state, recovering from a poisoned lock (it holds only plain
/// data and task handles, safe to reuse after a prior panic).
fn lock(mutex: &Mutex<Inner>) -> std::sync::MutexGuard<'_, Inner> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
