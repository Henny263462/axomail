//! Message-action commands: move, copy, archive, delete-to-Trash, and undo
//! (track 3.2 / M2).
//!
//! Every action follows the write-back shape established by
//! [`writeback`](crate::commands::writeback): apply to local storage **first**
//! (optimistic — the UI reads the new location back at once), then for a
//! write-back-capable IMAP message enqueue a `pending_ops` row and trigger one
//! `OpReplay` per affected account. POP3 and local-only messages move locally
//! with no server op and no error. Bulk variants enqueue one op per message and
//! trigger at most one replay per account.
//!
//! Move/archive/delete return the per-message *source* mailbox so the caller
//! can offer an undo (move back). The op payload also records that source (see
//! `axomail_sync::op_payload`) so the server op selects where the message still
//! sits, unaffected by the optimistic local move. **Nothing here ever
//! hard-deletes** — delete is a move to the account's Trash.
//!
//! Split into [`relocate`] (move/archive/delete/undo) and [`copy`] to stay
//! under the file-size gate; this root owns the shared helpers and the Tauri
//! wrappers.

mod copy;
mod relocate;

use std::collections::BTreeSet;

use axomail_core::{
    AccountId, IncomingMailProvider, IncomingProtocol, MailError, MailboxId, MessageEnvelope,
    MessageId,
};
use axomail_storage::{NewPendingOp, Store};
use axomail_sync::{ProviderFactory, SyncEngine, WorkRequest, delete_op, move_op};

use crate::commands::map_command_error;
use crate::dto::{MessageMoveDto, MoveOutcomeDto};
use crate::state::AppState;

pub use copy::copy_messages_impl;
pub use relocate::{
    archive_messages_impl, delete_to_trash_impl, move_messages_impl, undo_move_impl,
};

/// Shorthand for a user-facing configuration error.
pub(super) fn config(message: &str) -> MailError {
    MailError::Configuration {
        message: message.to_owned(),
    }
}

/// The account whose write-back queue an op for `envelope` (sitting in
/// `source`) should target — `Some` only for an IMAP account with a server
/// identity. POP3 and local-only messages propagate nothing.
pub(super) async fn writeback_account(
    store: &Store,
    envelope: &MessageEnvelope,
    source: MailboxId,
) -> Result<Option<AccountId>, MailError> {
    if envelope.remote_id.is_none() {
        return Ok(None);
    }
    let Some(mailbox) = store.mailboxes().get(source).await? else {
        return Ok(None);
    };
    let Some(account) = store.accounts().get(mailbox.account).await? else {
        return Ok(None);
    };
    Ok((account.incoming.protocol == IncomingProtocol::Imap).then_some(account.id))
}

/// Relocates one message to `target` locally and, when propagation applies,
/// enqueues the matching server op (`delete` chooses `delete_op` over
/// `move_op`). Returns the undo source mailbox and the account that gained an
/// op, or `None` when the message is gone or already in `target`.
pub(super) async fn relocate_one(
    store: &Store,
    message_id: MessageId,
    target: MailboxId,
    delete: bool,
) -> Result<Option<(MailboxId, Option<AccountId>)>, MailError> {
    let Some(envelope) = store.messages().get(message_id).await? else {
        return Ok(None);
    };
    let source = envelope.mailbox;
    if source == target {
        return Ok(None);
    }
    store.messages().set_mailbox(message_id, target).await?;

    let account = writeback_account(store, &envelope, source).await?;
    if let Some(account_id) = account {
        let (kind, payload_json) = if delete {
            delete_op(source)
        } else {
            move_op(source, target)
        };
        store
            .pending_ops()
            .enqueue(&NewPendingOp {
                account_id,
                message_id,
                kind,
                payload_json,
            })
            .await?;
    }
    Ok(Some((source, account)))
}

/// Triggers one `OpReplay` per account (de-duplicated by the engine).
pub(super) async fn trigger_replays<P, F>(
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

/// Loads a message and its owning account id, or `None` if either is gone.
pub(super) async fn message_account(
    store: &Store,
    message_id: MessageId,
) -> Result<Option<(MessageEnvelope, AccountId)>, MailError> {
    let Some(envelope) = store.messages().get(message_id).await? else {
        return Ok(None);
    };
    let Some(mailbox) = store.mailboxes().get(envelope.mailbox).await? else {
        return Ok(None);
    };
    Ok(Some((envelope, mailbox.account)))
}

/// Moves a selection of messages into a folder.
#[tauri::command]
pub async fn move_messages(
    state: tauri::State<'_, AppState>,
    message_ids: Vec<i64>,
    target_mailbox_id: i64,
) -> Result<MoveOutcomeDto, String> {
    move_messages_impl(
        state.store(),
        state.engine(),
        message_ids,
        target_mailbox_id,
    )
    .await
    .map_err(map_command_error)
}

/// Copies a selection of messages into a folder.
#[tauri::command]
pub async fn copy_messages(
    state: tauri::State<'_, AppState>,
    message_ids: Vec<i64>,
    target_mailbox_id: i64,
) -> Result<(), String> {
    copy_messages_impl(
        state.store(),
        state.engine(),
        state.raw_root(),
        message_ids,
        target_mailbox_id,
    )
    .await
    .map_err(map_command_error)
}

/// Archives a selection of messages.
#[tauri::command]
pub async fn archive_messages(
    state: tauri::State<'_, AppState>,
    message_ids: Vec<i64>,
) -> Result<MoveOutcomeDto, String> {
    archive_messages_impl(state.store(), state.engine(), message_ids)
        .await
        .map_err(map_command_error)
}

/// Deletes a selection of messages to Trash.
#[tauri::command]
pub async fn delete_to_trash(
    state: tauri::State<'_, AppState>,
    message_ids: Vec<i64>,
) -> Result<MoveOutcomeDto, String> {
    delete_to_trash_impl(state.store(), state.engine(), message_ids)
        .await
        .map_err(map_command_error)
}

/// Undoes a prior move/archive/delete.
#[tauri::command]
pub async fn undo_move(
    state: tauri::State<'_, AppState>,
    moves: Vec<MessageMoveDto>,
) -> Result<(), String> {
    undo_move_impl(state.store(), state.engine(), moves)
        .await
        .map_err(map_command_error)
}
