//! Application-side control of the IMAP IDLE push supervisor.
//!
//! [`PushControl`] owns the process's single
//! [`PushSupervisor`](axomail_sync::PushSupervisor) and keeps its effective
//! enabled state in sync with two independent switches: the user's `push_enabled`
//! setting and the global Work Offline flag. Push runs only when the user has it
//! on **and** the app is online — offline means no network connections at all.
//!
//! Targets are "every IMAP account's inbox-role mailbox": IDLE watches the inbox
//! (where new mail lands) and a server-pushed change enqueues a normal sync.
//! POP3 accounts have no push and are skipped entirely; the poller keeps them
//! fresh.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use axomail_core::{IncomingProtocol, MailError, MailboxRole};
use axomail_storage::Store;
use axomail_sync::{PushConfig, PushSupervisor, PushTarget};

use crate::composition::{AnyIncomingProvider, AppProviderFactory};
use crate::settings;
use crate::state::AppSyncEngine;

/// The concrete push supervisor type wired for this application.
pub type AppPushSupervisor = PushSupervisor<AnyIncomingProvider, AppProviderFactory>;

/// Owns and manages the process's single [`PushSupervisor`].
pub struct PushControl {
    engine: Arc<AppSyncEngine>,
    store: Store,
    /// Shared Work Offline flag (also observed by the engine and scheduler).
    offline: Arc<AtomicBool>,
    inner: Mutex<Inner>,
}

/// Guarded state: the running supervisor and the user's push preference.
struct Inner {
    supervisor: Option<AppPushSupervisor>,
    user_enabled: bool,
}

impl PushControl {
    /// Creates a control that will drive `engine`, reading targets and the
    /// push setting from `store`. No supervisor runs until [`launch`](Self::launch).
    #[must_use]
    pub fn new(engine: Arc<AppSyncEngine>, store: Store, offline: Arc<AtomicBool>) -> Self {
        Self {
            engine,
            store,
            offline,
            inner: Mutex::new(Inner {
                supervisor: None,
                user_enabled: true,
            }),
        }
    }

    /// Starts (or restarts) the supervisor from the persisted `push_enabled`
    /// setting and the current inbox targets. Any previous supervisor is
    /// stopped. Starts effectively disabled while offline.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] if the setting or target set cannot be
    /// read.
    pub async fn launch(&self) -> Result<(), MailError> {
        let user_enabled = settings::push_enabled(&self.store).await?;
        let targets = inbox_push_targets(&self.store).await?;
        let effective = user_enabled && !self.offline.load(Ordering::SeqCst);
        let supervisor = PushSupervisor::start(
            Arc::clone(&self.engine),
            self.store.clone(),
            PushConfig::default(),
            effective,
            targets,
        );
        let mut inner = lock(&self.inner);
        inner.user_enabled = user_enabled;
        // Replacing drops the previous supervisor, whose `Drop` stops its tasks.
        inner.supervisor = Some(supervisor);
        Ok(())
    }

    /// Recomputes inbox targets and pushes them into the running supervisor.
    /// A no-op when no supervisor has launched yet.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] if the target set cannot be read.
    pub async fn refresh_targets(&self) -> Result<(), MailError> {
        let targets = inbox_push_targets(&self.store).await?;
        if let Some(supervisor) = lock(&self.inner).supervisor.as_ref() {
            supervisor.update_targets(targets);
        }
        Ok(())
    }

    /// Applies the user's push preference (the caller persists it first) and
    /// reconciles the supervisor's effective state.
    pub fn set_user_enabled(&self, enabled: bool) {
        let mut inner = lock(&self.inner);
        inner.user_enabled = enabled;
        let effective = enabled && !self.offline.load(Ordering::SeqCst);
        if let Some(supervisor) = inner.supervisor.as_ref() {
            supervisor.set_enabled(effective);
        }
    }

    /// Reconciles push against the Work Offline toggle: offline disables push
    /// (no connections), online restores it to the user's preference.
    pub fn set_offline(&self, offline: bool) {
        let inner = lock(&self.inner);
        let effective = inner.user_enabled && !offline;
        if let Some(supervisor) = inner.supervisor.as_ref() {
            supervisor.set_enabled(effective);
        }
    }

    /// The user's push preference (independent of the offline gate).
    #[must_use]
    pub fn is_user_enabled(&self) -> bool {
        lock(&self.inner).user_enabled
    }
}

/// Builds the push target set: one entry per IMAP account inbox mailbox.
///
/// POP3 accounts are skipped — they have no server push.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
async fn inbox_push_targets(store: &Store) -> Result<Vec<PushTarget>, MailError> {
    let mut targets = Vec::new();
    for account in store.accounts().list().await? {
        if account.incoming.protocol != IncomingProtocol::Imap {
            continue;
        }
        for mailbox in store.mailboxes().list(account.id).await? {
            if mailbox.role == MailboxRole::Inbox {
                targets.push(PushTarget {
                    account_id: account.id,
                    mailbox_id: mailbox.id,
                    remote_name: mailbox.remote_name.clone(),
                });
            }
        }
    }
    Ok(targets)
}

/// Locks the inner state, recovering from a poisoned lock (plain data + a task
/// handle, safe to reuse after a prior panic).
fn lock(mutex: &Mutex<Inner>) -> std::sync::MutexGuard<'_, Inner> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
