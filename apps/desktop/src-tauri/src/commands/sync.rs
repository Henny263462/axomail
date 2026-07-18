//! The `sync_account` command: enqueue a sync job for each syncable mailbox
//! and return the ids of the jobs the engine created.

use std::collections::HashSet;
use std::time::Duration;

use axomail_core::{AccountId, IncomingMailProvider, MailError, MailboxRole};
use axomail_storage::Store;
use axomail_sync::{ProviderFactory, SyncEngine, SyncEvent, SyncMailbox};
use tokio::sync::broadcast::error::RecvError;

use crate::commands::map_command_error;
use crate::dto::checked_id;
use crate::state::AppState;

/// Upper bound on how long the command waits to observe the queued-job
/// events for the mailboxes it enqueued. Fresh jobs are announced within
/// milliseconds; the cap only bounds the tail when some mailbox already had
/// an active job (de-duplicated by the engine, so it emits no event).
const COLLECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Whether a mailbox role is synchronized in the MVP.
///
/// Per the composer contract the inbox plus any `inbox`/`archive`/`custom`
/// mailbox is synced; `drafts`, `sent`, `trash`, and `junk` hold local or
/// non-incoming content and are skipped.
fn is_syncable(role: MailboxRole) -> bool {
    matches!(
        role,
        MailboxRole::Inbox | MailboxRole::Archive | MailboxRole::Custom
    )
}

/// Enqueues a sync job per syncable mailbox of `account_id` and returns the
/// ids of the jobs the engine created (in queue order).
///
/// De-duplication is the engine's job: a mailbox that already has a pending
/// or running job is not queued again and is absent from the returned ids.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the account does not exist,
/// [`MailError::Database`] on a query failure, and [`MailError::Cancelled`]
/// if the engine has shut down.
pub async fn sync_account_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    account_id: i64,
) -> Result<Vec<i64>, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let account = AccountId::new(account_id);
    if store.accounts().get(account).await?.is_none() {
        return Err(MailError::Configuration {
            message: "cannot sync an account that does not exist".to_owned(),
        });
    }

    let syncable: Vec<_> = store
        .mailboxes()
        .list(account)
        .await?
        .into_iter()
        .filter(|mailbox| is_syncable(mailbox.role))
        .map(|mailbox| mailbox.id)
        .collect();
    if syncable.is_empty() {
        return Ok(Vec::new());
    }

    // Subscribe before enqueuing so no `JobQueued` announcement is missed.
    let mut events = engine.subscribe();
    let mut pending: HashSet<i64> = syncable.iter().map(|id| id.value()).collect();
    for mailbox_id in &syncable {
        engine
            .enqueue(SyncMailbox {
                account_id: account,
                mailbox_id: *mailbox_id,
            })
            .await?;
    }

    let mut raw_ids: Vec<i64> = Vec::with_capacity(syncable.len());
    let collect = async {
        while !pending.is_empty() {
            match events.recv().await {
                Ok(SyncEvent::JobQueued {
                    job_id,
                    account_id: queued_account,
                    mailbox_id,
                }) => {
                    if queued_account == account && pending.remove(&mailbox_id.value()) {
                        raw_ids.push(job_id.value());
                    }
                }
                Ok(_) => {}
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
        }
    };
    // A timeout here just stops waiting for de-duplicated mailboxes; the ids
    // already collected are returned.
    let _ = tokio::time::timeout(COLLECT_TIMEOUT, collect).await;

    raw_ids.into_iter().map(checked_id).collect()
}

/// Enqueues sync jobs for the account's syncable mailboxes.
#[tauri::command]
pub async fn sync_account(
    state: tauri::State<'_, AppState>,
    account_id: i64,
) -> Result<Vec<i64>, String> {
    sync_account_impl(state.store(), state.engine(), account_id)
        .await
        .map_err(map_command_error)
}
