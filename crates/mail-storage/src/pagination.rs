//! Cursor-based pagination for message listings.
//!
//! Messages are ordered by `(date_unix DESC, id DESC)`. A keyset
//! [`MessageCursor`] captures the sort position of the last row of a page
//! so the next page can be fetched without an `OFFSET` scan, giving
//! stable results even as rows are inserted or removed concurrently.

use axomail_core::{MailError, MessageEnvelope};
use serde::{Deserialize, Serialize};

/// Sort value substituted for a `NULL` `date_unix` so undated messages
/// order deterministically after every dated message.
pub(crate) const NULL_DATE_SORT: i64 = i64::MIN;

/// Opaque keyset cursor marking a position in a message listing.
///
/// The fields are private on purpose: callers treat the cursor as an
/// opaque token, obtained from [`MessagePage::next`] and passed back to
/// [`crate::MessageRepo::list_page`]. [`MessageCursor::token`] and
/// [`MessageCursor::parse_token`] provide a compact string form for
/// carrying it across a process boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageCursor {
    /// Coalesced sort date of the last row of the previous page.
    sort_date: i64,
    /// Row id of the last row of the previous page.
    id: i64,
}

impl MessageCursor {
    /// Builds a cursor from a row's coalesced sort date and id.
    pub(crate) fn new(sort_date: i64, id: i64) -> Self {
        Self { sort_date, id }
    }

    /// The coalesced sort date this cursor points just past.
    pub(crate) fn sort_date(self) -> i64 {
        self.sort_date
    }

    /// The row id this cursor points just past.
    pub(crate) fn id(self) -> i64 {
        self.id
    }

    /// Encodes the cursor as an opaque 32-character hex token.
    #[must_use]
    pub fn token(self) -> String {
        format!(
            "{:016x}{:016x}",
            self.sort_date.cast_unsigned(),
            self.id.cast_unsigned()
        )
    }

    /// Decodes a token produced by [`MessageCursor::token`].
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the token is not a valid
    /// 32-character hex string.
    pub fn parse_token(token: &str) -> Result<Self, MailError> {
        let invalid = || MailError::Configuration {
            message: "invalid pagination cursor".to_owned(),
        };
        if token.len() != 32 {
            return Err(invalid());
        }
        let sort_date = u64::from_str_radix(&token[..16], 16).map_err(|_| invalid())?;
        let id = u64::from_str_radix(&token[16..], 16).map_err(|_| invalid())?;
        Ok(Self {
            sort_date: sort_date.cast_signed(),
            id: id.cast_signed(),
        })
    }
}

/// One page of message envelopes plus the cursor for the following page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagePage {
    /// Envelopes in this page, newest first.
    pub items: Vec<MessageEnvelope>,
    /// Cursor for the next page, or `None` if this is the last page.
    pub next: Option<MessageCursor>,
}
