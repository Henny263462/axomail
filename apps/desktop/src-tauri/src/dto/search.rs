//! Search-result DTO.

use axomail_core::{MailError, MessageEnvelope};
use axomail_storage::SearchHit;
use serde::Serialize;

use super::checked_id;

/// One full-text search hit joined with its message envelope.
///
/// The `snippet` carries the FTS `[`/`]` markers around matched terms and is
/// plain text by construction (the index holds decoded header/body text, no
/// markup); the frontend renders it as text, never as HTML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultDto {
    /// Matching message.
    pub message_id: i64,
    /// Mailbox the match lives in.
    pub mailbox_id: i64,
    /// Account the match belongs to.
    pub account_id: i64,
    /// Decoded subject, if any.
    pub subject: Option<String>,
    /// Display name of the first sender, or an empty string.
    pub from_name: String,
    /// Address of the first sender, or an empty string.
    pub from_address: String,
    /// Date header in Unix seconds (UTC); `None` when it was absent.
    pub date_unix: Option<i64>,
    /// Highlighted snippet of the matching text.
    pub snippet: String,
}

impl SearchResultDto {
    /// Builds a result from a search hit, its resolved envelope, and the
    /// owning account id, validating every id crossing to the frontend.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if any id is out of the safe
    /// integer range.
    pub fn from_parts(
        hit: &SearchHit,
        envelope: &MessageEnvelope,
        account_id: i64,
    ) -> Result<Self, MailError> {
        let sender = envelope.from.first();
        Ok(Self {
            message_id: checked_id(hit.message_id.value())?,
            mailbox_id: checked_id(hit.mailbox_id.value())?,
            account_id: checked_id(account_id)?,
            subject: envelope.subject.clone(),
            from_name: sender
                .and_then(|address| address.display_name.clone())
                .unwrap_or_default(),
            from_address: sender
                .map(|address| address.email.as_str().to_owned())
                .unwrap_or_default(),
            date_unix: envelope.date.map(axomail_core::UtcTimestamp::unix_seconds),
            snippet: hit.snippet.clone(),
        })
    }
}
