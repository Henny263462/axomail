//! Composer DTOs: compose input, drafts, outbox items, and send outcomes.

use axomail_core::MailError;
use axomail_storage::{DraftRecord, OutboxRecord, OutboxState};
use serde::{Deserialize, Serialize};

use super::checked_id;

/// Content of one composed message, as submitted by the composer.
///
/// Recipients are plain address strings; the send path parses and validates
/// them into [`axomail_core::EmailAddress`] values before any network I/O.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeInput {
    /// Account the message is sent from.
    pub account_id: i64,
    /// Sender identity chosen in the From picker, if any. When present it
    /// drives the From/Reply-To headers; when absent the account address is
    /// used.
    #[serde(default)]
    pub identity_id: Option<i64>,
    /// Primary recipients.
    pub to: Vec<String>,
    /// Carbon-copy recipients.
    pub cc: Vec<String>,
    /// Blind-carbon-copy recipients.
    pub bcc: Vec<String>,
    /// Subject line.
    pub subject: String,
    /// Plain-text body (always present; the mandatory alternative).
    pub text_body: String,
    /// Optional sanitized HTML body. When present and non-blank on an immediate
    /// send it is sent as the `multipart/alternative` HTML part; it is
    /// re-sanitized server-side before assembly and is **not** persisted (the
    /// durable outbox/draft is text-only), so a deferred send falls back to the
    /// text alternative.
    #[serde(default)]
    pub html_body: Option<String>,
    /// Absolute filesystem paths of files to attach, chosen through the native
    /// open-file dialog. The bytes are read by a Rust `std::fs` command at send
    /// time (never by the webview); the filename shown to recipients is
    /// sanitized. Attachments ride only on the immediate send path.
    #[serde(default)]
    pub attachments: Vec<String>,
    /// Unix second at which to send a "send later" message. When present and
    /// in the future, the message is queued and withheld until then rather
    /// than submitted immediately.
    #[serde(default)]
    pub scheduled_at_unix: Option<i64>,
}

/// Metadata for one candidate attachment, returned by `stat_attachments` so the
/// composer can show a name/size chip before the file is ever read into a
/// message. No bytes cross the IPC boundary here. (Distinct from
/// `detail::AttachmentMetaDto`, which describes a *received* message's parts.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCandidateDto {
    /// The absolute path the composer passes back at send time.
    pub path: String,
    /// Filename sanitized for display and for the eventual MIME part.
    pub filename: String,
    /// Size in bytes, as reported by the filesystem.
    pub size_bytes: u64,
}

/// A stored draft, as listed by `list_drafts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftDto {
    /// Local identifier.
    pub id: i64,
    /// Owning account.
    pub account_id: i64,
    /// Primary recipients.
    pub to: Vec<String>,
    /// Carbon-copy recipients.
    pub cc: Vec<String>,
    /// Blind-carbon-copy recipients.
    pub bcc: Vec<String>,
    /// Subject line.
    pub subject: String,
    /// Plain-text body.
    pub text_body: String,
    /// Last-updated timestamp (Unix seconds).
    pub updated_unix: i64,
}

impl DraftDto {
    /// Builds a draft DTO from a stored record, validating its ids.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the draft or account id is
    /// out of the safe integer range.
    pub fn from_record(record: DraftRecord) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(record.id.value())?,
            account_id: checked_id(record.account.value())?,
            to: record.to,
            cc: record.cc,
            bcc: record.bcc,
            subject: record.subject,
            text_body: record.body_text,
            updated_unix: record.updated_unix,
        })
    }
}

/// One row of the outbox, as listed by `list_outbox`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxItemDto {
    /// Local identifier.
    pub id: i64,
    /// Owning account.
    pub account_id: i64,
    /// Subject line.
    pub subject: String,
    /// Short human-readable recipient summary (e.g. `a@x, b@y`).
    pub to_summary: String,
    /// Lifecycle state (`queued` is surfaced as `pending`).
    pub state: &'static str,
    /// Number of submission attempts so far.
    pub attempts: i64,
    /// Redacted last failure reason, if any.
    pub last_error: Option<String>,
    /// Identity the message will be sent under, if any.
    pub identity_id: Option<i64>,
    /// Unix second a scheduled ("send later") message is withheld until, or
    /// `null` for an immediately-eligible row.
    pub scheduled_unix: Option<i64>,
    /// Last-updated timestamp (Unix seconds).
    pub updated_unix: i64,
}

impl OutboxItemDto {
    /// Builds an outbox DTO from a stored record, validating its ids.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the outbox, account, or
    /// identity id is out of the safe integer range.
    pub fn from_record(record: OutboxRecord) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(record.id.value())?,
            account_id: checked_id(record.account.value())?,
            subject: record.subject,
            to_summary: summarize(&record.to),
            state: state_str(record.state),
            attempts: record.attempts,
            last_error: record.last_error,
            identity_id: record
                .identity
                .map(|id| checked_id(id.value()))
                .transpose()?,
            scheduled_unix: record.next_attempt_unix,
            updated_unix: record.updated_unix,
        })
    }
}

/// The contract surfaces the storage `queued` state as `pending`.
fn state_str(state: OutboxState) -> &'static str {
    match state {
        OutboxState::Queued => "pending",
        OutboxState::Sending => "sending",
        OutboxState::Sent => "sent",
        OutboxState::Failed => "failed",
    }
}

/// Joins recipient addresses into a compact one-line summary.
fn summarize(recipients: &[String]) -> String {
    recipients.join(", ")
}

/// Result of a send, retry, or schedule attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendOutcomeDto {
    /// Outbox row the attempt acted on.
    pub outbox_id: i64,
    /// Whether the message was accepted by the submission server.
    pub sent: bool,
    /// Redacted failure reason when `sent` is false.
    pub error: Option<String>,
    /// When set, the message was not sent now but queued for this Unix second
    /// (a "send later" outcome); `sent` is false and `error` is `None`.
    pub scheduled_at_unix: Option<i64>,
    /// True when the message was queued because the app is working offline; it
    /// stays in the outbox and is sent automatically on returning online.
    /// `sent` is false and `error` is `None`.
    pub queued_offline: bool,
}

impl SendOutcomeDto {
    /// Builds an immediate send-outcome DTO, validating the outbox id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the outbox id is out of the
    /// safe integer range.
    pub fn new(outbox_id: i64, sent: bool, error: Option<String>) -> Result<Self, MailError> {
        Ok(Self {
            outbox_id: checked_id(outbox_id)?,
            sent,
            error,
            scheduled_at_unix: None,
            queued_offline: false,
        })
    }

    /// Builds a "scheduled for later" outcome, validating the outbox id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the outbox id is out of the
    /// safe integer range.
    pub fn scheduled(outbox_id: i64, at_unix: i64) -> Result<Self, MailError> {
        Ok(Self {
            outbox_id: checked_id(outbox_id)?,
            sent: false,
            error: None,
            scheduled_at_unix: Some(at_unix),
            queued_offline: false,
        })
    }

    /// Builds a "queued while offline" outcome, validating the outbox id. The
    /// message is in the outbox and sends automatically when the app is back
    /// online.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the outbox id is out of the
    /// safe integer range.
    pub fn queued_offline(outbox_id: i64) -> Result<Self, MailError> {
        Ok(Self {
            outbox_id: checked_id(outbox_id)?,
            sent: false,
            error: None,
            scheduled_at_unix: None,
            queued_offline: true,
        })
    }
}
