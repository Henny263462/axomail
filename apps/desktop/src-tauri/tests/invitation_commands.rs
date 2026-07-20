//! Integration tests for the iMIP invitation commands: the reading-pane card
//! enrichment (`get_message_detail`), the CANCEL side effect, and the RSVP
//! flow that records `my_status` and submits a `METHOD:REPLY` to the organizer
//! over a scripted TLS SMTP server.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;
use std::path::Path;

use axomail_core::{
    AccountId, Address, EmailAddress, MailboxId, MessageFlags, MessageId, UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::detail::get_message_detail_impl;
use axomail_desktop_lib::commands::invitations::respond_invitation_impl;
use axomail_desktop_lib::dto::RsvpChoice;
use axomail_desktop_lib::state::SharedSecretStore;
use axomail_storage::{NewInvite, NewMessage, RsvpStatus, Store};
use axomail_test_support::{ScriptedServer, Step, TestTlsIdentity};
use common::provider::StubFactory;
use common::{outgoing_account_input, secret_stores, seed_standard_folders, valid_input};

/// A multipart REQUEST invitation addressed to the account (Klaus), organized
/// by Ada, with a UTC start/end.
const REQUEST: &str = concat!(
    "From: Ada Lovelace <ada@example.org>\r\n",
    "To: Klaus <klaus@theferbers.eu>\r\n",
    "Subject: Invitation: Design sync\r\n",
    "Date: Sun, 19 Jul 2026 09:00:00 +0000\r\n",
    "Message-ID: <req-1@example.org>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: multipart/mixed; boundary=\"b\"\r\n",
    "\r\n",
    "--b\r\n",
    "Content-Type: text/plain; charset=utf-8\r\n",
    "\r\n",
    "You are invited to the design sync.\r\n",
    "--b\r\n",
    "Content-Type: text/calendar; method=REQUEST; charset=utf-8\r\n",
    "\r\n",
    "BEGIN:VCALENDAR\r\n",
    "METHOD:REQUEST\r\n",
    "VERSION:2.0\r\n",
    "BEGIN:VEVENT\r\n",
    "UID:evt-77@example.org\r\n",
    "SEQUENCE:0\r\n",
    "SUMMARY:Design sync\r\n",
    "DTSTART:20260720T110000Z\r\n",
    "DTEND:20260720T120000Z\r\n",
    "LOCATION:Room 5\r\n",
    "ORGANIZER;CN=Ada Lovelace:mailto:ada@example.org\r\n",
    "ATTENDEE;CN=Klaus;PARTSTAT=NEEDS-ACTION;RSVP=TRUE:mailto:klaus@theferbers.eu\r\n",
    "END:VEVENT\r\n",
    "END:VCALENDAR\r\n",
    "--b--\r\n",
);

/// A CANCEL for the same UID as [`REQUEST`].
const CANCEL: &str = concat!(
    "From: Ada Lovelace <ada@example.org>\r\n",
    "To: Klaus <klaus@theferbers.eu>\r\n",
    "Subject: Cancelled: Design sync\r\n",
    "Date: Mon, 20 Jul 2026 08:00:00 +0000\r\n",
    "Message-ID: <cancel-1@example.org>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: text/calendar; method=CANCEL; charset=utf-8\r\n",
    "\r\n",
    "BEGIN:VCALENDAR\r\n",
    "METHOD:CANCEL\r\n",
    "BEGIN:VEVENT\r\n",
    "UID:evt-77@example.org\r\n",
    "SEQUENCE:1\r\n",
    "STATUS:CANCELLED\r\n",
    "END:VEVENT\r\n",
    "END:VCALENDAR\r\n",
);

/// A plain, non-invitation message.
const PLAIN: &str = concat!(
    "From: Ada Lovelace <ada@example.org>\r\n",
    "To: Klaus <klaus@theferbers.eu>\r\n",
    "Subject: Just a note\r\n",
    "Date: Sun, 19 Jul 2026 09:00:00 +0000\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: text/plain; charset=utf-8\r\n",
    "\r\n",
    "No calendar here.\r\n",
);

/// Writes `bytes` to `<raw_root>/raw/<message_id>.eml`, creating the dir.
fn cache_raw(raw_root: &Path, message_id: i64, bytes: &str) {
    let dir = raw_root.join("raw");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{message_id}.eml")), bytes).unwrap();
}

/// Inserts a local message row and caches its raw body.
async fn insert_with_raw(
    store: &Store,
    raw_root: &Path,
    mailbox: MailboxId,
    subject: &str,
    body: &str,
) -> MessageId {
    let message = NewMessage {
        mailbox,
        remote_id: None,
        subject: Some(subject.to_owned()),
        from: vec![Address {
            display_name: Some("Ada Lovelace".to_owned()),
            email: EmailAddress::parse("ada@example.org").unwrap(),
        }],
        to: vec![Address::from_email(
            EmailAddress::parse("klaus@theferbers.eu").unwrap(),
        )],
        cc: vec![],
        bcc: vec![],
        date: Some(UtcTimestamp::from_unix_seconds(1_752_912_000)),
        flags: MessageFlags::default(),
        size_bytes: Some(body.len() as u64),
        preview: None,
        has_raw: true,
    };
    let id = store.messages().insert(&message).await.unwrap().id;
    cache_raw(raw_root, id.value(), body);
    id
}

/// SMTP submission dialog capturing the DATA payload (single recipient).
fn smtp_steps() -> Vec<Step> {
    vec![
        Step::send_line("220 axomail.test ESMTP ready"),
        Step::expect_prefix("EHLO"),
        Step::send_line("250-axomail.test greets you"),
        Step::send_line("250-AUTH PLAIN"),
        Step::send_line("250 SIZE 52428800"),
        Step::expect_prefix("AUTH"),
        Step::send_line("235 2.7.0 authentication succeeded"),
        Step::expect_prefix("MAIL FROM"),
        Step::send_line("250 2.1.0 sender ok"),
        Step::expect_prefix("RCPT TO"),
        Step::send_line("250 2.1.5 recipient ok"),
        Step::expect_prefix("DATA"),
        Step::send_line("354 end data with <CR><LF>.<CR><LF>"),
        Step::expect_until("."),
        Step::send_line("250 2.0.0 accepted for delivery"),
        Step::expect_prefix("QUIT"),
        Step::send_line("221 2.0.0 bye"),
        Step::Close,
    ]
}

#[tokio::test]
async fn detail_of_a_request_carries_an_invitation_card() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let raw_root = tempfile::tempdir().unwrap();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = MailboxId::new(summary.mailboxes[0].id);
    let message = insert_with_raw(&store, raw_root.path(), inbox, "Invitation", REQUEST).await;

    let detail = get_message_detail_impl(
        &store,
        &StubFactory,
        raw_root.path(),
        message.value(),
        None,
        false,
    )
    .await
    .unwrap();

    let invitation = detail.invitation.expect("a request carries an invitation");
    assert_eq!(invitation.method, "request");
    assert_eq!(invitation.summary.as_deref(), Some("Design sync"));
    assert_eq!(
        invitation.organizer_email.as_deref(),
        Some("ada@example.org")
    );
    assert_eq!(invitation.location.as_deref(), Some("Room 5"));
    assert_eq!(invitation.my_status, "none");
    assert!(!invitation.cancelled);
    assert!(invitation.can_respond, "a live request is respondable");
    assert!(
        invitation
            .when_display
            .as_deref()
            .is_some_and(|when| when.contains("UTC")),
        "the UTC start is labeled: {:?}",
        invitation.when_display,
    );
}

#[tokio::test]
async fn a_plain_message_has_no_invitation() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let raw_root = tempfile::tempdir().unwrap();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = MailboxId::new(summary.mailboxes[0].id);
    let message = insert_with_raw(&store, raw_root.path(), inbox, "Note", PLAIN).await;

    let detail = get_message_detail_impl(
        &store,
        &StubFactory,
        raw_root.path(),
        message.value(),
        None,
        false,
    )
    .await
    .unwrap();
    assert!(detail.invitation.is_none(), "no calendar part, no card");
}

#[tokio::test]
async fn opening_a_cancel_marks_the_stored_invite_cancelled() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let raw_root = tempfile::tempdir().unwrap();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = MailboxId::new(summary.mailboxes[0].id);

    // A prior REQUEST the user accepted (a stored invite row for the UID).
    let request = insert_with_raw(&store, raw_root.path(), inbox, "Invitation", REQUEST).await;
    store
        .calendar_invites()
        .upsert(&NewInvite {
            message_id: request,
            uid: "evt-77@example.org".to_owned(),
            summary: Some("Design sync".to_owned()),
            organizer: Some("ada@example.org".to_owned()),
            starts_at: Some(1_784_545_200),
            ends_at: None,
            location: None,
            my_status: RsvpStatus::Accepted,
            sequence: 0,
        })
        .await
        .unwrap();

    // Opening the CANCEL retires the stored invite for its UID.
    let cancel = insert_with_raw(&store, raw_root.path(), inbox, "Cancelled", CANCEL).await;
    let detail = get_message_detail_impl(
        &store,
        &StubFactory,
        raw_root.path(),
        cancel.value(),
        None,
        false,
    )
    .await
    .unwrap();

    let invitation = detail.invitation.expect("a cancel carries an invitation");
    assert_eq!(invitation.method, "cancel");
    assert!(invitation.cancelled);
    assert!(!invitation.can_respond, "a cancel offers no response");

    let stored = store
        .calendar_invites()
        .get_by_message(request)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.cancelled, "the prior invite is now cancelled");
    assert_eq!(stored.my_status, RsvpStatus::Accepted, "response preserved");
}

#[tokio::test]
async fn rsvp_accept_records_status_and_sends_reply_to_organizer() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets): (_, SharedSecretStore) = secret_stores();
    let raw_root = tempfile::tempdir().unwrap();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let summary = create_account_impl(&store, &secrets, outgoing_account_input(server.port()))
        .await
        .unwrap();
    seed_standard_folders(&store, AccountId::new(summary.id)).await;
    let inbox = MailboxId::new(summary.mailboxes[0].id);
    let message = insert_with_raw(&store, raw_root.path(), inbox, "Invitation", REQUEST).await;

    let rsvp = respond_invitation_impl(
        &store,
        &StubFactory,
        &secrets,
        raw_root.path(),
        &roots,
        message.value(),
        RsvpChoice::Accept,
        false,
    )
    .await
    .unwrap();
    assert!(rsvp.submission.outcome.sent, "the reply should send");

    // The response is recorded only after the reply left.
    let stored = store
        .calendar_invites()
        .get_by_message(message)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.my_status, RsvpStatus::Accepted);
    assert_eq!(stored.uid, "evt-77@example.org");

    // The organizer received a valid METHOD:REPLY carrying the ACCEPTED PARTSTAT.
    let transcript = server.transcripts().into_iter().next().unwrap();
    let received = transcript.received_text();
    assert!(
        received.contains("RCPT TO:<ada@example.org>"),
        "reply addressed to the organizer: {received}"
    );
    assert!(
        received.contains("text/calendar"),
        "a calendar part is present: {received}"
    );
    assert!(
        received.contains("method=REPLY"),
        "the calendar Content-Type carries the reply method: {received}"
    );
    assert!(
        received.contains("METHOD:REPLY"),
        "the reply method is present: {received}"
    );
    assert!(
        received.contains("PARTSTAT=ACCEPTED"),
        "the accepted partstat is present: {received}"
    );
    assert!(
        received.contains("mailto:klaus@theferbers.eu"),
        "the responder is the attendee: {received}"
    );
}

#[tokio::test]
async fn rsvp_on_a_plain_message_is_rejected() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let raw_root = tempfile::tempdir().unwrap();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = MailboxId::new(summary.mailboxes[0].id);
    let message = insert_with_raw(&store, raw_root.path(), inbox, "Note", PLAIN).await;

    let result = respond_invitation_impl(
        &store,
        &StubFactory,
        &secrets,
        raw_root.path(),
        &[],
        message.value(),
        RsvpChoice::Accept,
        false,
    )
    .await;
    assert!(matches!(
        result,
        Err(axomail_core::MailError::Configuration { .. })
    ));
}
