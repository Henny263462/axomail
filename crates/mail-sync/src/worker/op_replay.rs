//! The `OpReplay` executor: drain an account's `pending_ops` queue to the
//! server (track 2.1, write-back).
//!
//! # Shape
//!
//! Like [`mailbox_sync`](super::mailbox_sync), this runs under the account
//! lock, permit, and claim already taken by [`run_job`](super::run_job). It
//! opens one connection and drains the queue, then a connection-level
//! transient triggers reconnect-with-backoff up to the configured budget.
//!
//! # Ordering & wedge-avoidance
//!
//! Ops are claimed as per-message heads in id order
//! ([`PendingOpsRepo::claim_next_batch`](axomail_storage::PendingOpsRepo::claim_next_batch)),
//! so a message's ops always replay in order while different messages
//! interleave. Each claimed op is attempted **at most once per drain**, and its
//! outcome decides the message chain's fate:
//!
//! - **success** → the op is completed (deleted); the chain advances and its
//!   next op is claimed on the following round.
//! - **temporary error** → the op is left `inflight` for the rest of this drain
//!   (which freezes only *its* message chain — an inflight head blocks later
//!   ops for the same message but nothing else) and returned to `pending` at
//!   the end, to be retried on a future replay run.
//! - **permanent / user-configuration error** → the op is parked (`failed`),
//!   which blocks its own chain until an operator requeues it but never stalls
//!   other messages.
//!
//! A poison op therefore wedges one message, never the account — the invariant
//! the queue is built around. Crash recovery for ops a previous run left
//! `inflight` is handled once at engine start (see `engine::recover`).

use axomail_core::{
    Account, AccountId, ErrorClass, IncomingMailProvider, MailError, MailboxId, MailboxName,
    MailboxRole,
};
use axomail_storage::{PendingOp, PendingOpId, PendingOpKind, SyncJob};
use tokio_util::sync::CancellationToken;

use crate::backoff::backoff_delay;
use crate::event::{SyncEvent, SyncJobSummary};
use crate::factory::ProviderFactory;
use crate::op_payload;
use crate::shared::Shared;

use super::guarded;

/// How many message heads to claim per drain round.
const OP_BATCH: u32 = 100;

/// Resolves the account, then drives the reconnect-with-backoff drain loop.
pub(super) async fn execute<P, F>(
    shared: &Shared<P, F>,
    job: &SyncJob,
    account_id: AccountId,
    token: &CancellationToken,
) -> Result<SyncJobSummary, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let account = shared
        .store
        .accounts()
        .get(account_id)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "account for op-replay job no longer exists".to_owned(),
        })?;
    // The Trash destination for delete (move-to-Trash) ops, resolved once.
    let trash = trash_mailbox_name(shared, account_id).await?;
    run_with_retry(shared, job, &account, account_id, &trash, token).await
}

/// Retries the whole drain on connection-level transient failures with backoff,
/// up to the configured attempt budget. Permanent, configuration, and
/// cancellation errors fail immediately; per-op errors never reach here.
async fn run_with_retry<P, F>(
    shared: &Shared<P, F>,
    job: &SyncJob,
    account: &Account,
    account_id: AccountId,
    trash: &Option<MailboxName>,
    token: &CancellationToken,
) -> Result<SyncJobSummary, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mut transient_failures = 0u32;
    loop {
        if token.is_cancelled() {
            return Err(MailError::Cancelled);
        }
        match run_drain(shared, job, account, account_id, trash, token).await {
            Ok(summary) => return Ok(summary),
            Err(error) => {
                if error.class() != ErrorClass::Temporary {
                    return Err(error);
                }
                transient_failures += 1;
                if transient_failures >= shared.config.max_attempts {
                    return Err(error);
                }
                let delay = backoff_delay(transient_failures, &shared.config);
                tokio::select! {
                    biased;
                    () = token.cancelled() => return Err(MailError::Cancelled),
                    () = tokio::time::sleep(delay) => {}
                }
            }
        }
    }
}

/// One connection lifecycle: build a provider, drain, and disconnect (best
/// effort). Connection-level errors propagate so the caller can retry.
async fn run_drain<P, F>(
    shared: &Shared<P, F>,
    job: &SyncJob,
    account: &Account,
    account_id: AccountId,
    trash: &Option<MailboxName>,
    token: &CancellationToken,
) -> Result<SyncJobSummary, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let net = shared.config.network_timeout;
    let mut provider = guarded(token, net, shared.factory.make(account)).await?;
    let outcome = drain(shared, job, account_id, trash, &mut provider, token).await;
    let _ = guarded(token, net, provider.disconnect()).await;
    outcome
}

/// Connects, then claims and applies pending ops until the queue is drained
/// (see the module docs for the per-op outcome rules).
async fn drain<P, F>(
    shared: &Shared<P, F>,
    job: &SyncJob,
    account_id: AccountId,
    trash: &Option<MailboxName>,
    provider: &mut P,
    token: &CancellationToken,
) -> Result<SyncJobSummary, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    guarded(token, shared.config.network_timeout, provider.connect()).await?;

    let ops = shared.store.pending_ops();
    let mut replayed = 0u64;
    let mut failed = 0u64;
    let mut progressed = false;
    let mut cancelled = false;
    // Currently-selected source mailbox, to skip redundant SELECTs.
    let mut selected: Option<MailboxId> = None;
    // Ops left `inflight` by a transient error this drain; returned to
    // `pending` after the drain so a future run retries them.
    let mut deferred: Vec<(PendingOpId, String)> = Vec::new();

    'drain: loop {
        if token.is_cancelled() {
            cancelled = true;
            break;
        }
        let batch = ops.claim_next_batch(account_id, OP_BATCH).await?;
        if batch.is_empty() {
            break;
        }
        for op in batch {
            match apply_one(shared, provider, &mut selected, trash, &op, token).await {
                Ok(()) => {
                    ops.complete(op.id).await?;
                    replayed += 1;
                    progressed = true;
                }
                Err(MailError::Cancelled) => {
                    // Leave-inflight then flush: return it to pending below.
                    deferred.push((op.id, MailError::Cancelled.to_string()));
                    cancelled = true;
                    break 'drain;
                }
                Err(error) if error.class() == ErrorClass::Temporary => {
                    // Freeze this message's chain for the rest of the drain by
                    // leaving the op `inflight`; requeue it at the end.
                    deferred.push((op.id, error.to_string()));
                }
                Err(error) => {
                    // Poison op: park it. Its chain blocks; other messages flow.
                    ops.fail(op.id, &error.to_string()).await?;
                    failed += 1;
                }
            }
        }
    }

    // Return every deferred (transiently-failed or cancelled-mid-flight) op to
    // `pending` so it is not stranded `inflight` for the rest of the session.
    for (id, message) in &deferred {
        ops.retry(*id, message).await?;
    }

    if cancelled {
        return Err(MailError::Cancelled);
    }
    // No progress plus outstanding transients means the connection is likely
    // dead: surface a temporary error so `run_with_retry` reconnects+backs off.
    if !progressed && !deferred.is_empty() {
        return Err(MailError::Network {
            message: "operation-queue replay made no progress; will retry".to_owned(),
        });
    }

    shared.emit(SyncEvent::OpsReplayed {
        job_id: job.id,
        account_id,
        replayed,
        failed,
    });
    // Op counts travel on `OpsReplayed`; the generic job summary stays zeroed.
    Ok(SyncJobSummary::default())
}

/// Applies one claimed op to the server. A message that is gone locally or has
/// no remote identity (a local-only draft) is a silent success (nothing to
/// replay). Every network round-trip is bounded by [`guarded`].
///
/// The mailbox selected before each op differs by kind: flag/keyword ops act on
/// the message's *live* mailbox (`envelope.mailbox`), while move/copy/delete act
/// on the **source** mailbox recorded in the payload at enqueue time. The
/// command layer applies a move optimistically to local storage, so by replay
/// `envelope.mailbox` is already the destination; selecting the recorded source
/// keeps the server op pointed at where the message physically still sits (see
/// `op_payload`).
async fn apply_one<P, F>(
    shared: &Shared<P, F>,
    provider: &mut P,
    selected: &mut Option<MailboxId>,
    trash: &Option<MailboxName>,
    op: &PendingOp,
    token: &CancellationToken,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let net = shared.config.network_timeout;
    let Some(envelope) = shared.store.messages().get(op.message_id).await? else {
        return Ok(());
    };
    let Some(remote_id) = envelope.remote_id.clone() else {
        return Ok(());
    };

    match op.kind {
        PendingOpKind::AddFlags
        | PendingOpKind::RemoveFlags
        | PendingOpKind::AddKeyword
        | PendingOpKind::RemoveKeyword => {
            let delta = op_payload::flag_delta(op)?;
            if delta.is_empty() {
                return Ok(());
            }
            ensure_selected(shared, provider, selected, envelope.mailbox, token).await?;
            guarded(token, net, provider.store_flags(&remote_id, &delta)).await
        }
        PendingOpKind::Move => {
            let relocation = op_payload::relocation(op)?;
            let target = mailbox_name(shared, relocation.target).await?;
            ensure_selected(shared, provider, selected, relocation.source, token).await?;
            if relocation.copy {
                guarded(token, net, provider.copy_message(&remote_id, &target)).await
            } else {
                guarded(token, net, provider.move_message(&remote_id, &target)).await
            }
        }
        PendingOpKind::Delete => {
            let source = op_payload::delete_source(op)?;
            let target = trash.clone().ok_or_else(|| MailError::Configuration {
                message: "account has no Trash mailbox for a delete op".to_owned(),
            })?;
            ensure_selected(shared, provider, selected, source, token).await?;
            guarded(token, net, provider.move_message(&remote_id, &target)).await
        }
    }
}

/// Selects `mailbox` on the provider unless it is already the selected one.
async fn ensure_selected<P, F>(
    shared: &Shared<P, F>,
    provider: &mut P,
    selected: &mut Option<MailboxId>,
    mailbox: MailboxId,
    token: &CancellationToken,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    if *selected == Some(mailbox) {
        return Ok(());
    }
    let name = mailbox_name(shared, mailbox).await?;
    guarded(
        token,
        shared.config.network_timeout,
        provider.select_mailbox(&name),
    )
    .await?;
    *selected = Some(mailbox);
    Ok(())
}

/// Resolves a mailbox's server name, erroring if the row is gone.
async fn mailbox_name<P, F>(
    shared: &Shared<P, F>,
    mailbox: MailboxId,
) -> Result<MailboxName, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    shared
        .store
        .mailboxes()
        .get(mailbox)
        .await?
        .map(|mailbox| mailbox.name)
        .ok_or_else(|| MailError::Configuration {
            message: "mailbox for pending op no longer exists".to_owned(),
        })
}

/// Finds the account's Trash mailbox name, if it has one.
async fn trash_mailbox_name<P, F>(
    shared: &Shared<P, F>,
    account_id: AccountId,
) -> Result<Option<MailboxName>, MailError>
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
        .find(|mailbox| mailbox.role == MailboxRole::Trash)
        .map(|mailbox| mailbox.name))
}
