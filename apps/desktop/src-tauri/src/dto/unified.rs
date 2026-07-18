//! Unified-inbox DTOs: a per-mailbox list item tagged with its account, and
//! the page wrapper carrying the next-page cursor.

use axomail_core::MailError;
use axomail_storage::{UnifiedInboxPage, UnifiedMessage};
use serde::Serialize;

use super::MessageListItemDto;

/// A unified-inbox row: a message list item plus the account it belongs to.
///
/// The list-item fields are flattened, so the serialized shape is exactly a
/// [`MessageListItemDto`] with one extra `accountId` field — matching the
/// frontend `UnifiedItemDto extends MessageListItem` contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedItemDto {
    /// The message envelope fields, identical to a per-mailbox listing.
    #[serde(flatten)]
    pub item: MessageListItemDto,
    /// Account whose inbox the message lives in.
    pub account_id: i64,
}

impl UnifiedItemDto {
    /// Builds a unified item from a storage row, validating its ids.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if any id is out of the safe
    /// integer range.
    pub fn from_unified(message: &UnifiedMessage) -> Result<Self, MailError> {
        Ok(Self {
            item: MessageListItemDto::from_envelope(&message.envelope)?,
            account_id: super::checked_id(message.account.value())?,
        })
    }
}

/// One page of the unified inbox plus an opaque cursor for the next page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedPageDto {
    /// Rows in this page, newest first across all accounts.
    pub items: Vec<UnifiedItemDto>,
    /// Opaque cursor for the next page, or `None` on the last page.
    pub next_cursor: Option<String>,
}

impl UnifiedPageDto {
    /// Maps a storage page to its DTO form, validating every id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if any id is out of the safe
    /// integer range.
    pub fn from_page(page: &UnifiedInboxPage) -> Result<Self, MailError> {
        let items = page
            .items
            .iter()
            .map(UnifiedItemDto::from_unified)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            items,
            next_cursor: page.next.map(|cursor| cursor.token()),
        })
    }
}
