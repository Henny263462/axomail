//! Run-on-arrival filtering: the post-ingest hook that applies an account's
//! filters to newly-arrived inbox messages.
//!
//! Called at the end of a successful inbox [`MailboxSync`](super::mailbox_sync).
//! Only the inbox triggers it (filters act on inbound mail); the per-account
//! high-water mark (see [`axomail_storage::FilterRepo`]) keeps each message from
//! being reprocessed, and baselining the mark on first encounter keeps enabling
//! filtering from retroactively sweeping the existing inbox. Any write-back ops
//! the actions enqueue are drained by a fresh `OpReplay` triggered here.
//!
//! A failure never fails the sync job — the caller logs and moves on.

use std::sync::Arc;

use axomail_core::{AccountId, IncomingMailProvider, MailError, MailboxId, MailboxRole};
use axomail_storage::{JobKind, NewSyncJob};

use crate::factory::ProviderFactory;
use crate::filter_run::run_filters_on_messages;
use crate::shared::Shared;

/// Applies arrival filters to any messages that reached `synced_mailbox` since
/// the last pass, when it is the account's inbox.
pub(super) async fn run<P, F>(
    shared: &Arc<Shared<P, F>>,
    account_id: AccountId,
    synced_mailbox: MailboxId,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let Some(inbox) = inbox_mailbox(shared, account_id).await? else {
        return Ok(());
    };
    if inbox != synced_mailbox {
        return Ok(());
    }

    let filters = shared.store.filters();
    let frontier = filters.max_message_id_in_mailbox(inbox).await?;
    let Some(state) = filters.arrival_state(account_id).await? else {
        // First encounter: baseline to the current frontier and filter nothing,
        // so turning filtering on never sweeps the pre-existing inbox.
        filters.baseline_arrival(account_id, frontier).await?;
        return Ok(());
    };
    if !state.enabled {
        return Ok(());
    }

    let candidates = filters
        .messages_in_mailbox_after(inbox, state.last_message_id)
        .await?;
    if candidates.is_empty() {
        return Ok(());
    }

    let report = run_filters_on_messages(&shared.store, account_id, &candidates).await?;
    // Advance past everything considered this pass (frontier captured the max
    // inbox id before any action moved a message out).
    filters.advance_arrival(account_id, frontier).await?;

    for account in report.replay_accounts {
        trigger_op_replay(shared, account).await;
    }
    Ok(())
}

/// The account's inbox mailbox id, if it has one.
async fn inbox_mailbox<P, F>(
    shared: &Arc<Shared<P, F>>,
    account_id: AccountId,
) -> Result<Option<MailboxId>, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    Ok(shared
        .store
        .mailboxes()
        .list(account_id)
        .await?
        .into_iter()
        .find(|mailbox| mailbox.role == MailboxRole::Inbox)
        .map(|mailbox| mailbox.id))
}

/// Enqueues a de-duplicated `OpReplay` for `account` and wakes the dispatcher,
/// so the write-back ops the filter actions queued are pushed to the server.
async fn trigger_op_replay<P, F>(shared: &Arc<Shared<P, F>>, account: AccountId)
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let new = NewSyncJob {
        kind: JobKind::OpReplay,
        account_id: Some(account),
        mailbox_id: None,
    };
    match shared.store.sync_jobs().create_if_absent(&new).await {
        Ok(Some(_)) => shared.resume.notify_one(),
        Ok(None) => {}
        Err(error) => {
            tracing::error!(error = %error, "failed to enqueue op-replay after arrival filtering");
        }
    }
}
