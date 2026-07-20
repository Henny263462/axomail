//! Read-receipt request header (`Disposition-Notification-To`, RFC 8098) on a
//! rendered outgoing message. Kept as an integration test so `mime.rs` stays
//! under the file-size gate.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axomail_core::{Address, EmailAddress, OutgoingMessage};
use axomail_smtp::render_message;

/// A minimal message with no threading and no receipt request.
fn base_message() -> OutgoingMessage {
    OutgoingMessage {
        from: Address::from_email(EmailAddress::parse("ada@example.org").unwrap()),
        to: vec![Address::from_email(
            EmailAddress::parse("bob@example.org").unwrap(),
        )],
        cc: vec![],
        bcc: vec![],
        reply_to: None,
        subject: "Hello".to_owned(),
        text_body: "Body.".to_owned(),
        html_body: None,
        attachments: vec![],
        in_reply_to: None,
        references: vec![],
        disposition_notification_to: None,
    }
}

fn rendered(message: &OutgoingMessage) -> String {
    String::from_utf8(render_message(message).unwrap().bytes).unwrap()
}

#[test]
fn writes_disposition_notification_to_when_a_receipt_is_requested() {
    let mut message = base_message();
    message.disposition_notification_to = Some("ada@example.org".to_owned());
    let out = rendered(&message);
    assert!(
        out.contains("Disposition-Notification-To: ada@example.org"),
        "read-receipt request header missing: {out}"
    );
}

#[test]
fn omits_disposition_notification_to_by_default() {
    let out = rendered(&base_message());
    assert!(
        !out.contains("Disposition-Notification-To:"),
        "unexpected read-receipt header: {out}"
    );
}
