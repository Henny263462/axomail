//! Message-list DTOs.

use axomail_core::{MailError, MessageEnvelope};
use axomail_storage::MessagePage;
use serde::Serialize;

use super::checked_id;

/// A single row in a message list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageListItemDto {
    /// Local identifier.
    pub id: i64,
    /// Owning mailbox.
    pub mailbox_id: i64,
    /// Decoded subject, if any.
    pub subject: Option<String>,
    /// Display name of the first sender, or an empty string.
    pub from_name: String,
    /// Address of the first sender, or an empty string.
    pub from_address: String,
    /// Date header in Unix seconds (UTC); `None` when it was absent.
    pub date_unix: Option<i64>,
    /// Whether the message has been read.
    pub seen: bool,
    /// Whether the message is flagged/starred.
    pub flagged: bool,
    /// Sanitized plain-text preview, no markup.
    pub preview: Option<String>,
}

impl MessageListItemDto {
    /// Builds a list item from a stored envelope, validating its ids.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the message or mailbox id is
    /// out of the safe integer range.
    pub fn from_envelope(envelope: &MessageEnvelope) -> Result<Self, MailError> {
        let sender = envelope.from.first();
        Ok(Self {
            id: checked_id(envelope.id.value())?,
            mailbox_id: checked_id(envelope.mailbox.value())?,
            subject: envelope.subject.clone(),
            from_name: sender
                .and_then(|address| address.display_name.clone())
                .unwrap_or_default(),
            from_address: sender
                .map(|address| address.email.as_str().to_owned())
                .unwrap_or_default(),
            date_unix: envelope.date.map(axomail_core::UtcTimestamp::unix_seconds),
            seen: envelope.flags.seen,
            flagged: envelope.flags.flagged,
            preview: envelope.preview.clone(),
        })
    }
}

/// One page of a message listing plus an opaque cursor for the next page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageListPageDto {
    /// Message rows, newest first.
    pub items: Vec<MessageListItemDto>,
    /// Opaque cursor token for the next page, or `None` on the last page.
    pub next_cursor: Option<String>,
}

impl MessageListPageDto {
    /// Maps a storage page to its DTO form, validating every id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if any id is out of the safe
    /// integer range.
    pub fn from_page(page: &MessagePage) -> Result<Self, MailError> {
        let items = page
            .items
            .iter()
            .map(MessageListItemDto::from_envelope)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            items,
            next_cursor: page.next.map(|cursor| cursor.token()),
        })
    }
}
