//! iCalendar (iMIP) invitation commands.
//!
//! Two surfaces: [`detail_invitation`] enriches a message detail with the
//! reading-pane invitation card (and, for a `CANCEL`, marks the stored event
//! off), and [`respond_invitation`] records an RSVP and sends an iMIP `REPLY`
//! email to the organizer through the Wave 3 compose plumbing.
//!
//! The client stores no calendar: only the RSVP state (migration 0018) is kept,
//! and the card presentation is re-derived from the message each time it opens.

mod reply;

use std::path::Path;

use axomail_core::{IncomingMailProvider, MailError, MessageId};
use axomail_parser::{CalendarMethod, ParsedMessage, extract_invitation, parse_message};
use axomail_storage::Store;
use axomail_sync::ProviderFactory;

use self::reply::prepare_rsvp;
use crate::commands::compose::{Submission, send_message_impl, spawn_sent_copy_append};
use crate::commands::detail::load_raw_for_export;
use crate::commands::map_command_error;
use crate::dto::{InvitationDto, InvitationResponseDto, RsvpChoice};
use crate::state::{AppState, SharedSecretStore};

/// Builds the reading-pane invitation card for a parsed message, if it carries
/// a `text/calendar` invitation.
///
/// A `CANCEL` for the event's UID marks every stored invitation for that UID
/// cancelled (best-effort). The card's `myStatus`/`cancelled` come from stored
/// RSVP state; the rest is derived from the message.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] only if the message id is out of the
/// JavaScript-safe integer range; store read failures degrade to the unanswered
/// state rather than failing the whole message load.
pub(crate) async fn detail_invitation(
    store: &Store,
    message_id: i64,
    parsed: &ParsedMessage,
) -> Result<Option<InvitationDto>, MailError> {
    let Some(invitation) = extract_invitation(parsed) else {
        return Ok(None);
    };

    // A cancellation retires every stored invitation sharing the UID. Best
    // effort: a failure here must not stop the message from rendering.
    if invitation.method == CalendarMethod::Cancel
        && let Err(error) = store
            .calendar_invites()
            .mark_cancelled_by_uid(&invitation.event.uid)
            .await
    {
        tracing::warn!(%message_id, %error, "failed to mark invitation cancelled");
    }

    let stored = match store
        .calendar_invites()
        .get_by_message(MessageId::new(message_id))
        .await
    {
        Ok(row) => row,
        Err(error) => {
            tracing::warn!(%message_id, %error, "failed to read stored invitation state");
            None
        }
    };
    let my_status = stored.as_ref().map_or("none", |row| row.my_status.as_str());
    let cancelled = stored.as_ref().is_some_and(|row| row.cancelled);

    InvitationDto::from_invitation(message_id, &invitation, my_status, cancelled).map(Some)
}

/// The outcome of an RSVP: the send [`Submission`] (outcome + any Sent-copy
/// upload plan) and the choice that was recorded.
pub struct InvitationRsvp {
    /// The send result funneled through the compose pipeline.
    pub submission: Submission,
    /// The choice that was requested.
    pub choice: RsvpChoice,
}

/// Records an RSVP for the invitation carried by a message and sends the iMIP
/// `REPLY` to the organizer.
///
/// The reply is submitted through the shared compose pipeline (the calendar
/// rides as a `text/calendar` part); the stored `my_status` is written only
/// once the reply has actually left, so a failed send never claims a response
/// the organizer did not receive.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] when the message is not a live meeting
/// `REQUEST`, has no organizer, or cannot be responded to while offline (an
/// RSVP must be sent); propagates parse, network, filesystem, and database
/// errors otherwise.
// Coordinates the fetch side (store/factory/raw_root) and the send side
// (secrets/extra_roots) plus the request itself; grouping them into a context
// struct would only move the arity into the caller, so the plain signature is
// clearer. `extra_roots` is empty in production (tests inject a local root).
#[allow(clippy::too_many_arguments)]
pub async fn respond_invitation_impl<P, F>(
    store: &Store,
    factory: &F,
    secrets: &SharedSecretStore,
    raw_root: &Path,
    extra_roots: &[Vec<u8>],
    message_id: i64,
    choice: RsvpChoice,
    offline: bool,
) -> Result<InvitationRsvp, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    // An RSVP must reach the organizer, so it cannot be recorded offline; reject
    // up front with a clear reason rather than after building the reply.
    if offline {
        return Err(MailError::Configuration {
            message: "an RSVP has to be sent, so it can't be recorded while working offline"
                .to_owned(),
        });
    }
    let bytes = load_raw_for_export(store, factory, raw_root, message_id, offline).await?;
    let parsed = parse_message(&bytes)?;
    let invitation = extract_invitation(&parsed).ok_or_else(|| MailError::Configuration {
        message: "this message is not a calendar invitation".to_owned(),
    })?;
    if invitation.method != CalendarMethod::Request {
        return Err(MailError::Configuration {
            message: "only a meeting request can be responded to".to_owned(),
        });
    }
    if invitation.event.is_cancelled() {
        return Err(MailError::Configuration {
            message: "this event has been cancelled".to_owned(),
        });
    }

    let envelope = store
        .messages()
        .get(MessageId::new(message_id))
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the invitation message no longer exists".to_owned(),
        })?;
    let mailbox = store
        .mailboxes()
        .get(envelope.mailbox)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the invitation's mailbox no longer exists".to_owned(),
        })?;
    let account = store
        .accounts()
        .get(mailbox.account)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the invitation's account no longer exists".to_owned(),
        })?;
    let identities = store.identities().list(account.id).await?;

    let prepared = prepare_rsvp(
        &invitation,
        &account,
        &identities,
        choice,
        message_id,
        now_unix(),
    )?;

    // Send through the Wave 3 compose plumbing. An offline send that carries the
    // calendar part is rejected up front there — an RSVP cannot be queued.
    let submission =
        send_message_impl(store, secrets, extra_roots, prepared.input, offline).await?;

    // Record the response only once the reply left, so `my_status` never claims
    // a notification the organizer did not receive.
    if submission.outcome.sent {
        store.calendar_invites().upsert(&prepared.invite).await?;
    }

    Ok(InvitationRsvp { submission, choice })
}

/// Current time in Unix seconds, or `0` if the clock predates the epoch.
fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Records an RSVP and sends the iMIP reply to the organizer.
///
/// On a successful send that resolved a server Sent folder, the Sent copy is
/// uploaded on a background task (mirroring `send_message`).
#[tauri::command]
pub async fn respond_invitation(
    state: tauri::State<'_, AppState>,
    message_id: i64,
    choice: RsvpChoice,
) -> Result<InvitationResponseDto, String> {
    let rsvp = respond_invitation_impl(
        state.store(),
        state.factory(),
        state.secrets(),
        state.raw_root(),
        &[],
        message_id,
        choice,
        state.is_offline(),
    )
    .await
    .map_err(map_command_error)?;

    if let Some(plan) = rsvp.submission.sent_copy {
        spawn_sent_copy_append(state.store(), state.factory(), plan);
    }
    let sent = rsvp.submission.outcome.sent;
    Ok(InvitationResponseDto {
        my_status: if sent {
            rsvp.choice.status_token().to_owned()
        } else {
            "none".to_owned()
        },
        sent,
        error: rsvp.submission.outcome.error.clone(),
    })
}
