//! Message Disposition Notification (MDN) assembly (RFC 8098).
//!
//! When a received message carried a `Disposition-Notification-To` request and
//! the configured receipt policy decides to honour it, [`render_mdn`] builds the
//! `multipart/report; report-type=disposition-notification` message that is sent
//! back to the requester. The report has the two RFC 8098 parts: a
//! human-readable `text/plain` explanation and the machine-readable
//! `message/disposition-notification` with the `Disposition` field.

use axomail_core::{Address, MailError};
use mail_builder::MessageBuilder;
use mail_builder::headers::address::Address as BuilderAddress;
use mail_builder::headers::content_type::ContentType;
use mail_builder::headers::message_id::MessageId;
use mail_builder::headers::raw::Raw;
use mail_builder::mime::MimePart;

use crate::mime::{RenderedMessage, generate_message_id};

/// How the disposition (the receipt) was produced: automatically per a standing
/// "always send" policy, or by an explicit user action ("ask" then confirm).
///
/// This selects the RFC 8098 `disposition-mode`: `automatic-action` +
/// `MDN-sent-automatically`, or `manual-action` + `MDN-sent-manually`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MdnDisposition {
    /// Sent automatically (the user configured "always send").
    Automatic,
    /// Sent after an explicit user confirmation.
    Manual,
}

impl MdnDisposition {
    /// The RFC 8098 `disposition-mode` token pair for this disposition, used as
    /// the left-hand side of the `Disposition:` field
    /// (`<action-mode>/<sending-mode>`).
    fn mode(self) -> &'static str {
        match self {
            Self::Automatic => "automatic-action/MDN-sent-automatically",
            Self::Manual => "manual-action/MDN-sent-manually",
        }
    }
}

/// Everything needed to build one read-receipt report.
#[derive(Debug, Clone)]
pub struct MdnReport {
    /// The sender of the receipt: the recipient of the original message (one of
    /// the account's own identities).
    pub from: Address,
    /// Where the receipt goes: the original message's
    /// `Disposition-Notification-To` address.
    pub to: Address,
    /// Subject of the original message (empty if it had none), used to build the
    /// `Read: <subject>` receipt subject.
    pub subject: String,
    /// The original message's `Message-ID` without angle brackets, echoed into
    /// the report's `Original-Message-ID` field and threaded via `In-Reply-To`.
    pub original_message_id: Option<String>,
    /// The receiving address (`Final-Recipient`), i.e. the identity that read
    /// the message — bare address, no display name.
    pub final_recipient: String,
    /// Whether the receipt was produced automatically or by user action.
    pub disposition: MdnDisposition,
}

/// A short, secret-free product token for the `Reporting-UA` field.
const REPORTING_UA: &str = "axomail";

/// Renders an RFC 8098 disposition-notification report, returning the RFC 5322
/// byte stream and the generated `Message-ID`.
///
/// The report is a `multipart/report; report-type=disposition-notification`
/// carrying a `text/plain` explanation and a `message/disposition-notification`
/// part whose `Disposition` field records `displayed`. `In-Reply-To` threads it
/// with the original message and `Auto-Submitted: auto-replied` (RFC 3834)
/// marks it as machine-generated so it does not itself trigger receipts or
/// vacation replies.
///
/// # Errors
///
/// Returns [`MailError::Protocol`] if serialization fails (an internal writer
/// error; not expected for in-memory buffers).
pub fn render_mdn(report: &MdnReport) -> Result<RenderedMessage, MailError> {
    let message_id = generate_message_id(report.from.email.domain());
    let human = human_readable(report);
    let machine = disposition_notification(report);

    let root = MimePart::new(
        ContentType::new("multipart/report").attribute("report-type", "disposition-notification"),
        vec![
            MimePart::new(ContentType::new("text/plain"), human),
            // A `message/*` part must use 7bit/8bit, never base64 (RFC 2045
            // §6.4); the content is ASCII, so 7bit is written verbatim.
            MimePart::new(
                ContentType::new("message/disposition-notification"),
                machine.into_bytes(),
            )
            .transfer_encoding("7bit"),
        ],
    );

    let mut builder = MessageBuilder::new()
        .from(builder_address(&report.from))
        .to(builder_address(&report.to))
        .subject(receipt_subject(&report.subject))
        .message_id(message_id.clone())
        .header("Auto-Submitted", Raw::new("auto-replied"))
        .body(root);
    if let Some(original) = &report.original_message_id {
        builder = builder.in_reply_to(MessageId::new(original.clone()));
    }

    let bytes = builder.write_to_vec().map_err(|err| MailError::Protocol {
        message: format!("failed to serialize disposition notification: {err}"),
    })?;
    Ok(RenderedMessage { bytes, message_id })
}

/// Builds the `Read: <subject>` receipt subject, handling an empty original
/// subject.
fn receipt_subject(subject: &str) -> String {
    if subject.trim().is_empty() {
        "Read: (no subject)".to_owned()
    } else {
        format!("Read: {subject}")
    }
}

/// The human-readable first part of the report.
fn human_readable(report: &MdnReport) -> String {
    format!(
        "This is a return receipt for the message you sent to {}.\n\n\
         Note: This receipt only acknowledges that the message was displayed \
         on the recipient's computer. There is no guarantee that the recipient \
         has read or understood the message.\n",
        report.final_recipient
    )
}

/// The machine-readable `message/disposition-notification` body (RFC 8098 §3.1).
///
/// Lines are CRLF-terminated per RFC 5322. `Original-Message-ID` is emitted only
/// when the original message declared a `Message-ID`.
fn disposition_notification(report: &MdnReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("Reporting-UA: {REPORTING_UA}; {REPORTING_UA}\r\n"));
    out.push_str(&format!(
        "Final-Recipient: rfc822;{}\r\n",
        report.final_recipient
    ));
    if let Some(original) = &report.original_message_id {
        out.push_str(&format!("Original-Message-ID: <{original}>\r\n"));
    }
    out.push_str(&format!(
        "Disposition: {}; displayed\r\n",
        report.disposition.mode()
    ));
    out
}

/// Converts a core [`Address`] into a mail-builder address, preserving any
/// display name.
fn builder_address(address: &Address) -> BuilderAddress<'_> {
    BuilderAddress::new_address(address.display_name.as_deref(), address.email.as_str())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use axomail_core::{Address, EmailAddress};

    use super::{MdnDisposition, MdnReport, render_mdn};

    fn report() -> MdnReport {
        MdnReport {
            from: Address::from_email(EmailAddress::parse("me@example.org").unwrap()),
            to: Address::from_email(EmailAddress::parse("sender@example.net").unwrap()),
            subject: "Quarterly plan".to_owned(),
            original_message_id: Some("orig-1@example.net".to_owned()),
            final_recipient: "me@example.org".to_owned(),
            disposition: MdnDisposition::Manual,
        }
    }

    fn rendered(report: &MdnReport) -> String {
        String::from_utf8(render_mdn(report).unwrap().bytes).unwrap()
    }

    #[test]
    fn builds_a_multipart_report_with_the_two_required_parts() {
        let out = rendered(&report());
        assert!(
            out.contains("multipart/report")
                && out.contains("report-type=\"disposition-notification\""),
            "expected a report content type: {out}"
        );
        assert!(
            out.contains("message/disposition-notification"),
            "expected the machine-readable part: {out}"
        );
        assert!(
            out.contains("Content-Type: text/plain"),
            "expected the human-readable part: {out}"
        );
    }

    #[test]
    fn records_the_disposition_and_original_fields() {
        let out = rendered(&report());
        assert!(
            out.contains("Final-Recipient: rfc822;me@example.org"),
            "final recipient missing: {out}"
        );
        assert!(
            out.contains("Original-Message-ID: <orig-1@example.net>"),
            "original message id missing: {out}"
        );
        assert!(
            out.contains("Disposition: manual-action/MDN-sent-manually; displayed"),
            "disposition field missing/wrong: {out}"
        );
    }

    #[test]
    fn addresses_the_receipt_to_the_requester_and_threads_it() {
        let out = rendered(&report());
        assert!(
            out.contains("To: <sender@example.net>"),
            "receipt To wrong: {out}"
        );
        assert!(
            out.contains("Subject: Read: Quarterly plan"),
            "subject wrong: {out}"
        );
        assert!(
            out.contains("In-Reply-To: <orig-1@example.net>"),
            "should thread with the original: {out}"
        );
        assert!(
            out.contains("Auto-Submitted: auto-replied"),
            "should mark itself auto-replied: {out}"
        );
    }

    #[test]
    fn automatic_disposition_uses_the_automatic_mode() {
        let mut report = report();
        report.disposition = MdnDisposition::Automatic;
        let out = rendered(&report);
        assert!(
            out.contains("Disposition: automatic-action/MDN-sent-automatically; displayed"),
            "automatic disposition mode missing: {out}"
        );
    }

    #[test]
    fn message_disposition_notification_is_not_base64() {
        // RFC 2045 §6.4: a message/* part must not be base64.
        let out = rendered(&report());
        let after = out
            .split("message/disposition-notification")
            .nth(1)
            .unwrap_or("");
        assert!(
            !after.contains("Content-Transfer-Encoding: base64"),
            "disposition-notification must not be base64: {out}"
        );
    }
}
