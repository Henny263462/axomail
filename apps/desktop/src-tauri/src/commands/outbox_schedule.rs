//! Send-later outbox controls: `send_outbox_now` and `cancel_scheduled_send`.
//!
//! A scheduled message is an ordinary `queued` outbox row whose
//! `next_attempt_unix` is in the future. "Send now" drops that schedule and
//! runs the normal guarded send flow; "cancel schedule" lifts the message back
//! out of the outbox into a draft the user can keep editing.

use axomail_core::MailError;
use axomail_storage::{NewDraft, OutboxId, Store};

use crate::commands::compose::retry_outbox_impl;
use crate::commands::map_command_error;
use crate::dto::{SendOutcomeDto, checked_id};
use crate::state::{AppState, SharedSecretStore};

/// Sends a scheduled (or otherwise queued) outbox row immediately: clears any
/// pending schedule, then runs the guarded send flow.
///
/// The guard inside [`retry_outbox_impl`] still applies, so a row already
/// `sending`/`sent` is never submitted twice.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the outbox row or its account no
/// longer exists.
pub async fn send_outbox_now_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
    outbox_id: i64,
) -> Result<SendOutcomeDto, MailError> {
    store
        .outbox()
        .clear_schedule(OutboxId::new(outbox_id))
        .await?;
    retry_outbox_impl(store, secrets, extra_roots, outbox_id).await
}

/// Cancels a scheduled send, returning its content to a new draft and removing
/// the outbox row. Only a `queued` (pending) row can be cancelled; a message
/// already in flight or sent cannot. Returns the new draft's id.
///
/// Cancellation is atomic: the row is removed with a `state = 'queued'` guard
/// **first**, and the draft is created only if that guarded delete claimed the
/// row. This closes the time-of-check/time-of-use race where a concurrent send
/// flips the row to `sending` between a plain state check and the delete — in
/// that case no draft is created and the in-flight (or sent) message is left
/// intact.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the row does not exist or is no
/// longer pending (already sending or sent), and a safe-integer error if the
/// new draft id cannot cross to the frontend.
pub async fn cancel_scheduled_send_impl(store: &Store, outbox_id: i64) -> Result<i64, MailError> {
    let id = OutboxId::new(outbox_id);
    // Read the content up front (also distinguishes "no such item" from "no
    // longer cancellable"); the guarded delete below is the real authority.
    let record = store
        .outbox()
        .get(id)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "no such outbox item".to_owned(),
        })?;

    // Atomic claim: only a still-queued row can be cancelled. If the guard
    // matches nothing, a send has already begun or completed — do not create a
    // draft.
    if !store.outbox().delete_if_queued(id).await? {
        return Err(MailError::Configuration {
            message: "this message is already sending or has been sent".to_owned(),
        });
    }

    let draft = store
        .drafts()
        .insert(&NewDraft {
            account: record.account,
            subject: record.subject,
            body_text: record.body_text,
            to: record.to,
            cc: record.cc,
            bcc: record.bcc,
        })
        .await?;
    checked_id(draft.id.value())
}

/// Sends a scheduled or queued outbox item immediately.
#[tauri::command]
pub async fn send_outbox_now(
    state: tauri::State<'_, AppState>,
    outbox_id: i64,
) -> Result<SendOutcomeDto, String> {
    send_outbox_now_impl(state.store(), state.secrets(), &[], outbox_id)
        .await
        .map_err(map_command_error)
}

/// Cancels a scheduled send, moving the message back to a draft.
#[tauri::command]
pub async fn cancel_scheduled_send(
    state: tauri::State<'_, AppState>,
    outbox_id: i64,
) -> Result<i64, String> {
    cancel_scheduled_send_impl(state.store(), outbox_id)
        .await
        .map_err(map_command_error)
}
