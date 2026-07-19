//! DTOs for the message-action commands (move / copy / archive / delete /
//! undo).
//!
//! A move, archive, or delete-to-Trash returns the list of relocations it
//! performed — each message's id and the mailbox it came *from* — so the
//! frontend can offer an undo that moves the messages back. Outbound ids route
//! through [`checked_id`](crate::dto::checked_id) so a value JavaScript could
//! not represent exactly never crosses the bridge. The undo input carries the
//! same pairs back; its ids are validated by lookup in the command.

use serde::{Deserialize, Serialize};

use axomail_core::MailError;

use crate::dto::checked_id;

/// One message relocation: the message and the mailbox it moved *from*.
///
/// Returned by move/archive/delete (so the caller can undo by moving back to
/// `from_mailbox_id`) and accepted back as the undo input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageMoveDto {
    /// The message that moved.
    pub message_id: i64,
    /// The mailbox it was in before the move (the undo target).
    pub from_mailbox_id: i64,
}

impl MessageMoveDto {
    /// Builds a validated relocation record, guarding both ids for the bridge.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if either id is out of the
    /// JavaScript-safe range.
    pub fn new(message_id: i64, from_mailbox_id: i64) -> Result<Self, MailError> {
        Ok(Self {
            message_id: checked_id(message_id)?,
            from_mailbox_id: checked_id(from_mailbox_id)?,
        })
    }
}

/// The result of a move/archive/delete: the relocations performed, in input
/// order. An empty list means nothing moved (every id was a no-op — already in
/// the target, or gone).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveOutcomeDto {
    /// Per-message relocations, each undoable by moving back to its
    /// `from_mailbox_id`.
    pub moves: Vec<MessageMoveDto>,
}

/// The result of a Compact: how many orphaned raw `.eml` blobs were reclaimed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactOutcomeDto {
    /// Number of orphaned raw bodies removed from disk.
    pub reclaimed: u64,
}
