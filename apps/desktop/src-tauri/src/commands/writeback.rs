//! Server-propagating flag commands (track 2.1, write-back).
//!
//! Every command here applies its change to local storage **first**
//! (optimistic; local storage is the single source of truth the UI reads back)
//! and then, for a write-back-capable IMAP account whose message has a server
//! identity, enqueues a `pending_ops` row and triggers an `OpReplay` job to
//! push it to the server. Bulk commands enqueue one op per message but trigger
//! at most one replay per account.
//!
//! POP3 has no server-side flags, so its changes are recorded locally only and
//! nothing is queued (the queue is never handed an op no adapter can satisfy).
//! Local-only messages (drafts, without a `remote_id`) likewise never enqueue.

use std::collections::BTreeSet;

use axomail_core::{
    AccountId, IncomingMailProvider, IncomingProtocol, MailError, MailboxId, MessageFlags,
    MessageId,
};
use axomail_storage::{NewPendingOp, PendingOpKind, Store};
use axomail_sync::{ProviderFactory, SyncEngine, WorkRequest, flagged_op, seen_op};

use crate::commands::map_command_error;
use crate::state::AppState;

/// Page size when scanning a mailbox for its unread messages.
const READ_SCAN_PAGE: u32 = 500;

/// A system flag this surface can toggle with server propagation.
#[derive(Clone, Copy)]
enum Flag {
    /// `\Seen` (read state).
    Seen,
    /// `\Flagged` (star).
    Flagged,
}

impl Flag {
    /// The flag's current value on `flags`.
    fn current(self, flags: &MessageFlags) -> bool {
        match self {
            Self::Seen => flags.seen,
            Self::Flagged => flags.flagged,
        }
    }

    /// Sets the flag's value on `flags`.
    fn set(self, flags: &mut MessageFlags, value: bool) {
        match self {
            Self::Seen => flags.seen = value,
            Self::Flagged => flags.flagged = value,
        }
    }

    /// The `(kind, payload)` of the write-back op for this flag change.
    fn op(self, value: bool) -> (PendingOpKind, String) {
        match self {
            Self::Seen => seen_op(value),
            Self::Flagged => flagged_op(value),
        }
    }
}

/// Applies `flag = value` to one message locally and, when propagation applies
/// (IMAP account, message has a server identity, value actually changed),
/// enqueues a write-back op. Returns the account whose queue gained an op.
async fn apply_flag(
    store: &Store,
    message_id: MessageId,
    flag: Flag,
    value: bool,
) -> Result<Option<AccountId>, MailError> {
    let Some(envelope) = store.messages().get(message_id).await? else {
        return Ok(None);
    };
    if flag.current(&envelope.flags) == value {
        // Already in the requested state: idempotent no-op, nothing to enqueue.
        return Ok(None);
    }
    let mut flags = envelope.flags.clone();
    flag.set(&mut flags, value);
    store.messages().update_flags(message_id, &flags).await?;

    // Server propagation needs a server identity and an IMAP account.
    if envelope.remote_id.is_none() {
        return Ok(None);
    }
    let Some(mailbox) = store.mailboxes().get(envelope.mailbox).await? else {
        return Ok(None);
    };
    let Some(account) = store.accounts().get(mailbox.account).await? else {
        return Ok(None);
    };
    if account.incoming.protocol != IncomingProtocol::Imap {
        return Ok(None);
    }

    let (kind, payload_json) = flag.op(value);
    store
        .pending_ops()
        .enqueue(&NewPendingOp {
            account_id: account.id,
            message_id,
            kind,
            payload_json,
        })
        .await?;
    Ok(Some(account.id))
}

/// Triggers one `OpReplay` per account (de-duplicated by the engine).
async fn trigger_replays<P, F>(
    engine: &SyncEngine<P, F>,
    accounts: BTreeSet<AccountId>,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    for account_id in accounts {
        engine
            .enqueue_work(WorkRequest::OpReplay { account_id })
            .await?;
    }
    Ok(())
}

/// Collects the ids of a mailbox's currently-unread messages.
async fn unread_ids(store: &Store, mailbox: MailboxId) -> Result<Vec<MessageId>, MailError> {
    let mut cursor = None;
    let mut ids = Vec::new();
    loop {
        let page = store
            .messages()
            .list_page(mailbox, cursor, READ_SCAN_PAGE)
            .await?;
        ids.extend(
            page.items
                .iter()
                .filter(|item| !item.flags.seen)
                .map(|item| item.id),
        );
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    Ok(ids)
}

/// Sets or clears `\Seen` on a message, applying locally and propagating.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Cancelled`] if the engine has shut down.
pub async fn mark_message_seen_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    message_id: i64,
    seen: bool,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let account = apply_flag(store, MessageId::new(message_id), Flag::Seen, seen).await?;
    trigger_replays(engine, account.into_iter().collect()).await
}

/// Sets or clears `\Flagged` (star) on a message, applying locally and
/// propagating.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Cancelled`] if the engine has shut down.
pub async fn set_message_flagged_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    message_id: i64,
    flagged: bool,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let account = apply_flag(store, MessageId::new(message_id), Flag::Flagged, flagged).await?;
    trigger_replays(engine, account.into_iter().collect()).await
}

/// Marks every unread message in a mailbox as read, enqueuing one op per
/// message and one replay per account. Returns how many messages were marked.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Cancelled`] if the engine has shut down.
pub async fn mark_mailbox_read_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    mailbox_id: i64,
) -> Result<u64, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let ids = unread_ids(store, MailboxId::new(mailbox_id)).await?;
    let marked = ids.len() as u64;
    let mut accounts = BTreeSet::new();
    for id in ids {
        if let Some(account_id) = apply_flag(store, id, Flag::Seen, true).await? {
            accounts.insert(account_id);
        }
    }
    trigger_replays(engine, accounts).await?;
    Ok(marked)
}

/// Sets the read state of a selection of messages (e.g. a multi-selection or a
/// thread), enqueuing one op per message and one replay per account.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Cancelled`] if the engine has shut down.
pub async fn mark_messages_read_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    message_ids: Vec<i64>,
    seen: bool,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mut accounts = BTreeSet::new();
    for id in message_ids {
        if let Some(account_id) = apply_flag(store, MessageId::new(id), Flag::Seen, seen).await? {
            accounts.insert(account_id);
        }
    }
    trigger_replays(engine, accounts).await
}

/// Sets or clears a message's read flag.
#[tauri::command]
pub async fn mark_message_seen(
    state: tauri::State<'_, AppState>,
    message_id: i64,
    seen: bool,
) -> Result<(), String> {
    mark_message_seen_impl(state.store(), state.engine(), message_id, seen)
        .await
        .map_err(map_command_error)
}

/// Sets or clears a message's star (flagged) flag.
#[tauri::command]
pub async fn set_message_flagged(
    state: tauri::State<'_, AppState>,
    message_id: i64,
    flagged: bool,
) -> Result<(), String> {
    set_message_flagged_impl(state.store(), state.engine(), message_id, flagged)
        .await
        .map_err(map_command_error)
}

/// Marks every unread message in a mailbox as read.
#[tauri::command]
pub async fn mark_mailbox_read(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
) -> Result<u64, String> {
    mark_mailbox_read_impl(state.store(), state.engine(), mailbox_id)
        .await
        .map_err(map_command_error)
}

/// Sets the read state of a selection of messages.
#[tauri::command]
pub async fn mark_messages_read(
    state: tauri::State<'_, AppState>,
    message_ids: Vec<i64>,
    seen: bool,
) -> Result<(), String> {
    mark_messages_read_impl(state.store(), state.engine(), message_ids, seen)
        .await
        .map_err(map_command_error)
}
