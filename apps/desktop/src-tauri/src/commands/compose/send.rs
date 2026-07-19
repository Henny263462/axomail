//! The `send_message` command: queue a composed message and (unless it is
//! scheduled or the app is offline) submit it immediately.

use axomail_core::{Account, MailError, OutgoingAttachment};
use axomail_security::sanitize_message_html;
use axomail_storage::{Identity, IdentityId, NewOutbox, Store};

use super::shared::{MessageExtras, load_account, now_unix, parse_addresses, submit_record};
use crate::commands::attachments::load_attachments;
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
pub async fn send_message_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
    input: ComposeInput,
    offline: bool,
) -> Result<SendOutcomeDto, MailError> {
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
    let deferred = scheduled.is_some() || offline;

    // The durable outbox row is text-only (no migration this wave). Attachments
    // therefore cannot survive to a later, worker-driven submission, so a
    // deferred send that carries them is rejected up front rather than sent
    // silently stripped. An HTML alternative degrades to the persisted text.
    if deferred && !input.attachments.is_empty() {
        return Err(MailError::Configuration {
            message: "attachments can only be sent now, not scheduled or queued while offline; \
                      remove them or send while connected"
                .to_owned(),
        });
    }

    // Read attachments and sanitize the HTML only for an immediate send. Reading
    // up front means an unreadable or oversized attachment fails the command
    // before any outbox row is written (like recipient validation).
    let extras = if deferred {
        MessageExtras::default()
    } else {
        MessageExtras {
            html_body: sanitize_html_body(input.html_body.as_deref()),
            attachments: read_attachments(input.attachments.clone()).await?,
        }
    };

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
        return SendOutcomeDto::scheduled(record.id.value(), at);
    }

    // Working offline: the message stays `queued` in the outbox and is sent by
    // the drain once the app is back online. No network I/O happens now.
    if offline {
        return SendOutcomeDto::queued_offline(record.id.value());
    }

    // A freshly created row is `queued`, so this transition always succeeds.
    if !store.outbox().begin_send(record.id).await? {
        return SendOutcomeDto::new(
            record.id.value(),
            false,
            Some("the message could not be queued for sending".to_owned()),
        );
    }
    submit_record(store, secrets, extra_roots, &account, &record, extras).await
}

/// Sanitizes a composer-supplied HTML body to the allowlisted subset before it
/// is sent (defense in depth over the editor's own client-side allowlist,
/// ADR-0007). A blank or empty-after-sanitizing body yields `None` so the
/// message stays text-only.
fn sanitize_html_body(html: Option<&str>) -> Option<String> {
    let raw = html?;
    if raw.trim().is_empty() {
        return None;
    }
    let cleaned = sanitize_message_html(raw).html;
    (!cleaned.trim().is_empty()).then_some(cleaned)
}

/// Reads attachment bytes off the async runtime's blocking pool (`std::fs` is
/// synchronous). Enforces the total size cap and sanitizes each filename.
///
/// # Errors
///
/// Propagates [`MailError::Configuration`] (over the size cap) or
/// [`MailError::Filesystem`] (unreadable file) from [`load_attachments`].
async fn read_attachments(paths: Vec<String>) -> Result<Vec<OutgoingAttachment>, MailError> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    tokio::task::spawn_blocking(move || load_attachments(&paths))
        .await
        .map_err(|error| MailError::Filesystem {
            message: format!("attachment read task failed: {error}"),
        })?
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
#[tauri::command]
pub async fn send_message(
    state: tauri::State<'_, AppState>,
    input: ComposeInput,
) -> Result<SendOutcomeDto, String> {
    send_message_impl(
        state.store(),
        state.secrets(),
        &[],
        input,
        state.is_offline(),
    )
    .await
    .map_err(map_command_error)
}
