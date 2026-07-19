//! Message-list DTOs.

use std::collections::HashMap;

use axomail_core::{MailError, MessageEnvelope};
use axomail_storage::{MessagePage, Tag};
use serde::Serialize;

use super::TagChipDto;
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
    /// Own `Message-ID` without angle brackets, or `None` when absent. Carried
    /// so the list can assemble conversation threads client-side (track 3.4).
    pub message_id: Option<String>,
    /// `In-Reply-To` message-ids in header order (usually one parent).
    pub in_reply_to: Vec<String>,
    /// `References` chain, oldest ancestor first. Empty until the threading
    /// columns are populated at ingest or backfilled on first open.
    pub references: Vec<String>,
    /// Tag chips for this row, in display order (empty when untagged, or when
    /// the caller does not attach tags — e.g. the unified inbox).
    pub tags: Vec<TagChipDto>,
}

impl MessageListItemDto {
    /// Builds a list item from a stored envelope, validating its ids. The row
    /// starts untagged; [`MessageListItemDto::with_tags`] attaches chips.
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
            message_id: envelope.threading.message_id.clone(),
            in_reply_to: envelope.threading.in_reply_to.clone(),
            references: envelope.threading.references.clone(),
            tags: Vec::new(),
        })
    }

    /// Attaches tag chips to this row (builder form).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if a tag id is out of the safe
    /// integer range.
    pub fn with_tags(mut self, tags: &[Tag]) -> Result<Self, MailError> {
        self.tags = tags
            .iter()
            .map(TagChipDto::from_tag)
            .collect::<Result<_, _>>()?;
        Ok(self)
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

    /// Maps a storage page to its DTO form, attaching tag chips from a
    /// message-id → tags map (built by one batched query, so the list stays
    /// O(page)). Rows absent from the map are simply untagged.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if any id is out of the safe
    /// integer range.
    pub fn from_page_with_tags(
        page: &MessagePage,
        tags: &HashMap<i64, Vec<Tag>>,
    ) -> Result<Self, MailError> {
        Ok(Self {
            items: items_with_tags(&page.items, tags)?,
            next_cursor: page.next.map(|cursor| cursor.token()),
        })
    }

    /// Builds a cursorless page from a flat list of envelopes plus a tag map,
    /// used by the tag-filter view (no pagination).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if any id is out of the safe
    /// integer range.
    pub fn from_envelopes_with_tags(
        envelopes: &[MessageEnvelope],
        tags: &HashMap<i64, Vec<Tag>>,
    ) -> Result<Self, MailError> {
        Ok(Self {
            items: items_with_tags(envelopes, tags)?,
            next_cursor: None,
        })
    }
}

/// Builds list items from envelopes, attaching each message's tags from the
/// map (untagged when absent).
fn items_with_tags(
    envelopes: &[MessageEnvelope],
    tags: &HashMap<i64, Vec<Tag>>,
) -> Result<Vec<MessageListItemDto>, MailError> {
    envelopes
        .iter()
        .map(|envelope| {
            let item = MessageListItemDto::from_envelope(envelope)?;
            match tags.get(&envelope.id.value()) {
                Some(tags) => item.with_tags(tags),
                None => Ok(item),
            }
        })
        .collect()
}
