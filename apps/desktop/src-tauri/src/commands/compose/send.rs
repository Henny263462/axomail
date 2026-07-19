//! The `send_message` command: queue a composed message and (unless it is
//! scheduled or the app is offline) submit it immediately.

use axomail_core::{Account, MailError};
use axomail_storage::{Identity, IdentityId, NewOutbox, Store};

use super::append::spawn_sent_copy_append;
use super::shared::{Submission, load_account, now_unix, parse_addresses, submit_record};
use crate::commands::map_command_error;
use crate::dto::{ComposeInput, SendOutcomeDto};
use crate::state::{AppState, SharedSecretStore};

/// Sends a composed message through the outbox.
///
/// Recipients are validated up front, so an invalid address fails the
/// command before any row is written. On success the outbox row is marked
/// `sent` and a local copy is filed in the account's Sent mailbox.
///
/// `extra_roots` is empty in production; tests inject a local server's
/// self-signed certificate through it.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an empty or invalid recipient
/// set or an unknown account. A submission failure is reported inside the
/// returned [`SendOutcomeDto`] (with a redacted reason), not as an error.
///
/// On a successful immediate send the returned [`Submission`] carries a
/// [`SentCopyPlan`](super::append::SentCopyPlan) for an IMAP account with a
/// resolvable Sent folder; the command wrapper uploads that copy in the
/// background.
pub async fn send_message_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
    input: ComposeInput,
    offline: bool,
) -> Result<Submission, MailError> {
    if input.to.is_empty() {
        return Err(MailError::Configuration {
            message: "a message needs at least one primary (To) recipient".to_owned(),
        });
    }
    // Validate every address before writing anything.
    parse_addresses(&input.to)?;
    parse_addresses(&input.cc)?;
    parse_addresses(&input.bcc)?;

    let account = load_account(store, input.account_id).await?;
    let identity = resolve_identity(store, &account, input.identity_id).await?;
    // A schedule strictly in the future defers the send; a past/now instant is
    // treated as "send now".
    let scheduled = input.scheduled_at_unix.filter(|&at| at > now_unix());

    let record = store
        .outbox()
        .create(&NewOutbox {
            account: account.id,
            draft_id: None,
            identity_id: identity.as_ref().map(|id| id.id),
            subject: input.subject,
            body_text: input.text_body,
            to: input.to,
            cc: input.cc,
            bcc: input.bcc,
            next_attempt_unix: scheduled,
        })
        .await?;

    // A scheduled message waits in the outbox; the worker's time guard keeps it
    // from being submitted until it is due.
    if let Some(at) = scheduled {
        return Ok(no_copy(SendOutcomeDto::scheduled(record.id.value(), at)?));
    }

    // Working offline: the message stays `queued` in the outbox and is sent by
    // the drain once the app is back online. No network I/O happens now.
    if offline {
        return Ok(no_copy(SendOutcomeDto::queued_offline(record.id.value())?));
    }

    // A freshly created row is `queued`, so this transition always succeeds.
    if !store.outbox().begin_send(record.id).await? {
        return Ok(no_copy(SendOutcomeDto::new(
            record.id.value(),
            false,
            Some("the message could not be queued for sending".to_owned()),
        )?));
    }
    submit_record(store, secrets, extra_roots, &account, &record).await
}

/// Wraps an outcome that has no Sent copy to upload (scheduled, offline, or a
/// pre-submission failure).
fn no_copy(outcome: SendOutcomeDto) -> Submission {
    Submission {
        outcome,
        sent_copy: None,
    }
}

/// Loads and validates the chosen sender identity, if any.
///
/// Returns `Ok(None)` when no identity was selected (the account address is
/// used). Errors if the identity does not exist or belongs to another account.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown or mismatched identity.
async fn resolve_identity(
    store: &Store,
    account: &Account,
    identity_id: Option<i64>,
) -> Result<Option<Identity>, MailError> {
    let Some(identity_id) = identity_id else {
        return Ok(None);
    };
    let identity = store
        .identities()
        .get(IdentityId::new(identity_id))
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the message references an identity that does not exist".to_owned(),
        })?;
    if identity.account != account.id {
        return Err(MailError::Configuration {
            message: "the chosen identity does not belong to the sending account".to_owned(),
        });
    }
    Ok(Some(identity))
}

/// Sends a composed message, queuing it in the outbox first.
///
/// On a successful send that resolved a server Sent folder, the Sent copy is
/// uploaded to the server on a background task before the outcome is returned,
/// so a slow IMAP `APPEND` never blocks the composer.
#[tauri::command]
pub async fn send_message(
    state: tauri::State<'_, AppState>,
    input: ComposeInput,
) -> Result<SendOutcomeDto, String> {
    let submission = send_message_impl(
        state.store(),
        state.secrets(),
        &[],
        input,
        state.is_offline(),
    )
    .await
    .map_err(map_command_error)?;
    if let Some(plan) = submission.sent_copy {
        spawn_sent_copy_append(state.store(), state.factory(), plan);
    }
    Ok(submission.outcome)
}
