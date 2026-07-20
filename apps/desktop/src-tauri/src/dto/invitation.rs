//! DTOs for iCalendar (iMIP) invitations: the reading-pane card payload, the
//! RSVP choice input, and the RSVP outcome.

use axomail_parser::{Invitation, format_when};
use serde::{Deserialize, Serialize};

use super::checked_id;
use axomail_core::MailError;

/// The RSVP choice a user makes on an invitation card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RsvpChoice {
    /// Accept the invitation (`PARTSTAT=ACCEPTED`).
    Accept,
    /// Respond tentatively (`PARTSTAT=TENTATIVE`).
    Tentative,
    /// Decline the invitation (`PARTSTAT=DECLINED`).
    Decline,
}

impl RsvpChoice {
    /// The iCalendar `PARTSTAT` value carried in the reply's attendee line.
    #[must_use]
    pub const fn partstat(self) -> &'static str {
        match self {
            Self::Accept => "ACCEPTED",
            Self::Tentative => "TENTATIVE",
            Self::Decline => "DECLINED",
        }
    }

    /// The persisted `my_status` token this choice records.
    #[must_use]
    pub const fn status_token(self) -> &'static str {
        match self {
            Self::Accept => "accepted",
            Self::Tentative => "tentative",
            Self::Decline => "declined",
        }
    }

    /// The subject-line verb for the reply email.
    #[must_use]
    pub const fn verb(self) -> &'static str {
        match self {
            Self::Accept => "Accepted",
            Self::Tentative => "Tentative",
            Self::Decline => "Declined",
        }
    }
}

/// The invitation card payload attached to a message detail.
///
/// The presentation fields are re-derived from the message on every open; only
/// `myStatus` and `cancelled` come from stored RSVP state. `canRespond` is the
/// single gate the card uses to show Accept/Tentative/Decline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationDto {
    /// Local id of the carrying message.
    pub message_id: i64,
    /// The event's iCalendar UID.
    pub uid: String,
    /// The iMIP method slug (`request`, `reply`, `cancel`, …).
    pub method: String,
    /// The event title.
    pub summary: Option<String>,
    /// The organizer's display name.
    pub organizer_name: Option<String>,
    /// The organizer's email.
    pub organizer_email: Option<String>,
    /// Human-readable, timezone-labeled start/end for display.
    pub when_display: Option<String>,
    /// Event start as Unix seconds (best-effort; exact only for UTC times).
    pub starts_at_unix: Option<i64>,
    /// Event end as Unix seconds.
    pub ends_at_unix: Option<i64>,
    /// The event location.
    pub location: Option<String>,
    /// The event agenda (`DESCRIPTION`).
    pub description: Option<String>,
    /// The user's stored response (`none` until they respond).
    pub my_status: String,
    /// Whether the event has been cancelled (by method or a stored CANCEL).
    pub cancelled: bool,
    /// Whether the card should offer Accept/Tentative/Decline.
    pub can_respond: bool,
}

impl InvitationDto {
    /// Builds the card DTO from a parsed invitation plus its stored RSVP state.
    ///
    /// `my_status` is the persisted response token (`none` when unanswered),
    /// `cancelled` the stored cancellation flag. The card offers a response
    /// only for a live `REQUEST` that carries an organizer to reply to.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the message id is outside the
    /// safe integer range.
    pub fn from_invitation(
        message_id: i64,
        invitation: &Invitation,
        my_status: &str,
        cancelled: bool,
    ) -> Result<Self, MailError> {
        let event = &invitation.event;
        let cancelled = cancelled
            || event.is_cancelled()
            || invitation.method == axomail_parser::CalendarMethod::Cancel;
        let organizer_email = event
            .organizer
            .as_ref()
            .map(|organizer| organizer.email.clone());
        let can_respond = invitation.method == axomail_parser::CalendarMethod::Request
            && !cancelled
            && organizer_email.is_some();
        Ok(Self {
            message_id: checked_id(message_id)?,
            uid: event.uid.clone(),
            method: invitation.method.slug(),
            summary: event.summary.clone(),
            organizer_name: event
                .organizer
                .as_ref()
                .and_then(|organizer| organizer.name.clone()),
            organizer_email,
            when_display: format_when(event.dtstart.as_ref(), event.dtend.as_ref()),
            starts_at_unix: event.dtstart.as_ref().map(|dt| dt.to_unix_seconds()),
            ends_at_unix: event.dtend.as_ref().map(|dt| dt.to_unix_seconds()),
            location: event.location.clone(),
            description: event.description.clone(),
            my_status: my_status.to_owned(),
            cancelled,
            can_respond,
        })
    }
}

/// The result of an RSVP: the new stored status and whether the reply was sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationResponseDto {
    /// The stored `my_status` after the response.
    pub my_status: String,
    /// Whether the iMIP reply email was successfully submitted.
    pub sent: bool,
    /// A redacted failure reason when the reply could not be sent.
    pub error: Option<String>,
}
