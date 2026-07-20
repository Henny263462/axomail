//! Pure assembly of an iMIP RSVP: the outgoing reply message and the RSVP row.
//!
//! Given a parsed `REQUEST` invitation, the account, and the chosen response,
//! this resolves who "me" is among the attendees, builds a `METHOD:REPLY`
//! iCalendar body carrying the responder's `PARTSTAT`, and packages it as a
//! [`ComposeInput`] addressed to the organizer — sent through the Wave 3
//! compose plumbing. No I/O happens here, so it is unit-tested directly.

use axomail_core::{Account, MailError, MessageId};
use axomail_parser::{Invitation, ReplyParams, build_reply};
use axomail_storage::{Identity, NewInvite, RsvpStatus};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::dto::{ComposeAttachmentDto, ComposeInput, RsvpChoice};

/// The Content-Type of the reply's calendar part: an iMIP `REPLY`, UTF-8.
const CALENDAR_MIME: &str = "text/calendar; method=REPLY; charset=utf-8; component=VEVENT";

/// A prepared RSVP: the reply to submit and the row recording the response.
pub(super) struct PreparedRsvp {
    /// The reply message to send through the compose pipeline.
    pub input: ComposeInput,
    /// The invitation row to persist once the reply is sent.
    pub invite: NewInvite,
}

/// Builds the reply message and RSVP row for a response to `invitation`.
///
/// `now_unix` seeds the reply's `DTSTAMP` (passed in so this stays pure).
///
/// # Errors
///
/// Returns [`MailError::Configuration`] when the invitation carries no
/// organizer to reply to.
pub(super) fn prepare_rsvp(
    invitation: &Invitation,
    account: &Account,
    identities: &[Identity],
    choice: RsvpChoice,
    message_id: i64,
    now_unix: i64,
) -> Result<PreparedRsvp, MailError> {
    let event = &invitation.event;
    let organizer = event
        .organizer
        .as_ref()
        .filter(|organizer| !organizer.email.trim().is_empty())
        .ok_or_else(|| MailError::Configuration {
            message: "this invitation has no organizer to reply to".to_owned(),
        })?;

    let responder = resolve_responder(event, account, identities);
    let reply_params = ReplyParams {
        responder_email: responder.email.clone(),
        responder_name: responder.name.clone(),
        partstat: choice.partstat().to_owned(),
        dtstamp_unix: now_unix,
    };
    let ics = build_reply(event, &reply_params);
    let calendar = ComposeAttachmentDto {
        filename: "invite.ics".to_owned(),
        mime_type: CALENDAR_MIME.to_owned(),
        content_base64: STANDARD.encode(ics.as_bytes()),
    };

    let title = event.summary.as_deref().unwrap_or("Invitation");
    let who = responder.name.as_deref().unwrap_or(&responder.email);
    let input = ComposeInput {
        account_id: account.id.value(),
        identity_id: responder.identity_id,
        to: vec![organizer.email.clone()],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: format!("{}: {title}", choice.verb()),
        text_body: format!(
            "{who} has {} this invitation.",
            choice.verb().to_ascii_lowercase()
        ),
        html_body: None,
        attachments: Vec::new(),
        scheduled_at_unix: None,
        in_reply_to: None,
        references: Vec::new(),
        // The reply rides as a text/calendar part through the shared attachment
        // path (base64 in, one multipart/mixed out).
        forwarded_attachments: vec![calendar],
        request_receipt: false,
    };

    let invite = NewInvite {
        message_id: MessageId::new(message_id),
        uid: event.uid.clone(),
        summary: event.summary.clone(),
        organizer: Some(organizer.email.clone()),
        starts_at: event.dtstart.as_ref().map(|dt| dt.to_unix_seconds()),
        ends_at: event.dtend.as_ref().map(|dt| dt.to_unix_seconds()),
        location: event.location.clone(),
        my_status: status_of(choice),
        sequence: event.sequence,
    };

    Ok(PreparedRsvp { input, invite })
}

/// Who is responding: the email written into the reply's `ATTENDEE`, the
/// display name, and the identity to send From (so the From matches the
/// attendee), all resolved against the event's attendees.
struct Responder {
    email: String,
    name: Option<String>,
    identity_id: Option<i64>,
}

/// Resolves the responder by matching the account's addresses against the
/// event's attendees; falls back to the account's own address when the invite
/// listed no address of ours (still a valid RSVP, sent as the account).
fn resolve_responder(
    event: &axomail_parser::CalendarEvent,
    account: &Account,
    identities: &[Identity],
) -> Responder {
    let self_emails = self_emails(account, identities);
    if let Some(attendee) = event.attendee_matching(&self_emails) {
        let identity = identities
            .iter()
            .find(|identity| identity.fields.email.eq_ignore_ascii_case(&attendee.email));
        let name = attendee
            .name
            .clone()
            .or_else(|| identity.map(|identity| identity.fields.display_name.clone()))
            .or_else(|| Some(account.display_name.clone()))
            .filter(|name| !name.trim().is_empty());
        Responder {
            email: attendee.email.clone(),
            name,
            identity_id: identity.map(|identity| identity.id.value()),
        }
    } else {
        Responder {
            email: account.email.as_str().to_owned(),
            name: (!account.display_name.trim().is_empty()).then(|| account.display_name.clone()),
            identity_id: None,
        }
    }
}

/// The account's own addresses (its email plus every identity's), lowercased.
fn self_emails(account: &Account, identities: &[Identity]) -> Vec<String> {
    let mut emails = vec![account.email.as_str().to_ascii_lowercase()];
    emails.extend(
        identities
            .iter()
            .map(|identity| identity.fields.email.to_ascii_lowercase()),
    );
    emails
}

/// Maps the UI choice to the persisted RSVP status.
fn status_of(choice: RsvpChoice) -> RsvpStatus {
    match choice {
        RsvpChoice::Accept => RsvpStatus::Accepted,
        RsvpChoice::Tentative => RsvpStatus::Tentative,
        RsvpChoice::Decline => RsvpStatus::Declined,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use axomail_core::{AccountId, EmailAddress};
    use axomail_parser::parse_calendar;

    use super::*;

    const REQUEST: &str = concat!(
        "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\n",
        "UID:evt-9@example.org\r\nSEQUENCE:1\r\nSUMMARY:Design sync\r\n",
        "DTSTART:20260720T110000Z\r\n",
        "ORGANIZER;CN=Ada:mailto:ada@example.org\r\n",
        "ATTENDEE;CN=Klaus;PARTSTAT=NEEDS-ACTION:mailto:klaus@theferbers.eu\r\n",
        "END:VEVENT\r\nEND:VCALENDAR\r\n",
    );

    fn account() -> Account {
        // A minimal account is awkward to hand-build; borrow the storage
        // fixture shape by parsing only what prepare_rsvp reads.
        use axomail_core::{
            AccountAuth, ConnectionSecurity, CredentialRef, IncomingProtocol, IncomingServerConfig,
            OutgoingServerConfig, ServerEndpoint,
        };
        use std::num::NonZeroU16;
        let port = |value: u16| ServerEndpoint::new("h", NonZeroU16::new(value).unwrap()).unwrap();
        Account {
            id: AccountId::new(3),
            display_name: "Klaus Ferber".to_owned(),
            email: EmailAddress::parse("klaus@theferbers.eu").unwrap(),
            auth: AccountAuth::Password,
            incoming: IncomingServerConfig {
                protocol: IncomingProtocol::Imap,
                endpoint: port(993),
                security: ConnectionSecurity::Tls,
                username: "klaus@theferbers.eu".to_owned(),
                credential: CredentialRef::new("axomail/account/incoming"),
            },
            outgoing: OutgoingServerConfig {
                endpoint: port(465),
                security: ConnectionSecurity::Tls,
                username: "klaus@theferbers.eu".to_owned(),
                credential: CredentialRef::new("axomail/account/outgoing"),
            },
        }
    }

    #[test]
    fn accept_addresses_the_organizer_and_records_accepted() {
        let invitation = parse_calendar(REQUEST).unwrap();
        let prepared = prepare_rsvp(
            &invitation,
            &account(),
            &[],
            RsvpChoice::Accept,
            42,
            1_784_545_200,
        )
        .unwrap();

        assert_eq!(prepared.input.to, vec!["ada@example.org"]);
        assert_eq!(prepared.input.subject, "Accepted: Design sync");
        assert_eq!(prepared.input.forwarded_attachments.len(), 1);
        let attachment = &prepared.input.forwarded_attachments[0];
        assert!(attachment.mime_type.starts_with("text/calendar"));
        let ics = String::from_utf8(STANDARD.decode(&attachment.content_base64).unwrap()).unwrap();
        assert!(ics.contains("METHOD:REPLY"));
        assert!(ics.contains("PARTSTAT=ACCEPTED"));
        assert!(ics.contains("mailto:klaus@theferbers.eu"));
        assert_eq!(prepared.invite.my_status, RsvpStatus::Accepted);
        assert_eq!(prepared.invite.uid, "evt-9@example.org");
        assert_eq!(
            prepared.invite.organizer.as_deref(),
            Some("ada@example.org")
        );
    }

    #[test]
    fn decline_writes_declined_partstat_and_status() {
        let invitation = parse_calendar(REQUEST).unwrap();
        let prepared =
            prepare_rsvp(&invitation, &account(), &[], RsvpChoice::Decline, 42, 0).unwrap();
        assert_eq!(prepared.input.subject, "Declined: Design sync");
        let ics = String::from_utf8(
            STANDARD
                .decode(&prepared.input.forwarded_attachments[0].content_base64)
                .unwrap(),
        )
        .unwrap();
        assert!(ics.contains("PARTSTAT=DECLINED"));
        assert_eq!(prepared.invite.my_status, RsvpStatus::Declined);
    }

    #[test]
    fn an_invitation_without_an_organizer_is_rejected() {
        let no_org = concat!(
            "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\n",
            "UID:x\r\nSUMMARY:No organizer\r\n",
            "ATTENDEE:mailto:klaus@theferbers.eu\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        );
        let invitation = parse_calendar(no_org).unwrap();
        let result = prepare_rsvp(&invitation, &account(), &[], RsvpChoice::Accept, 1, 0);
        assert!(matches!(result, Err(MailError::Configuration { .. })));
    }
}
