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
    /// Last-updated timestamp (Unix seconds).
    pub updated_unix: i64,
}

impl OutboxItemDto {
    /// Builds an outbox DTO from a stored record, validating its ids.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the outbox or account id is
    /// out of the safe integer range.
    pub fn from_record(record: OutboxRecord) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(record.id.value())?,
            account_id: checked_id(record.account.value())?,
            subject: record.subject,
            to_summary: summarize(&record.to),
            state: state_str(record.state),
            attempts: record.attempts,
            last_error: record.last_error,
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

/// Result of a send or retry attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendOutcomeDto {
    /// Outbox row the attempt acted on.
    pub outbox_id: i64,
    /// Whether the message was accepted by the submission server.
    pub sent: bool,
    /// Redacted failure reason when `sent` is false.
    pub error: Option<String>,
}

impl SendOutcomeDto {
    /// Builds a send-outcome DTO, validating the outbox id.
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
        })
    }
}
