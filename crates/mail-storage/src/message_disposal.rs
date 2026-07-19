//! Message relocation and bulk disposal for [`MessageRepo`].
//!
//! These are the storage side of the Wave 3 message actions and the Trash
//! lifecycle: the optimistic local move ([`set_mailbox`](MessageRepo::set_mailbox)),
//! and the bulk row deletions behind Empty Trash
//! ([`list_ids_in_mailbox`](MessageRepo::list_ids_in_mailbox) +
//! [`delete_in_mailbox`](MessageRepo::delete_in_mailbox)) and Compact
//! ([`all_ids`](MessageRepo::all_ids), which the caller diffs against the raw
//! `.eml` blobs on disk). Row deletes cascade to `message_recipients`,
//! `message_search_content` (which the FTS delete trigger then de-indexes),
//! `message_tags`, and `pending_ops` (all `ON DELETE CASCADE`), so no explicit
//! FTS/queue cleanup is needed here. The out-of-database raw `.eml` bodies are
//! keyed by message id and reclaimed by the command layer via the ids these
//! methods return.
//!
//! Kept in a sibling module (accessing `MessageRepo`'s crate-visible `pool`) so
//! `messages.rs` stays under the file-size gate.

use axomail_core::{MailError, MailboxId, MessageId};

use crate::error_map::map_sqlx;
use crate::messages::MessageRepo;

impl MessageRepo<'_> {
    /// Moves a message to `mailbox` locally (an optimistic move: the UI reads
    /// the new location back immediately, while the server op replays
    /// asynchronously). Returns whether the message exists.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set_mailbox(&self, id: MessageId, mailbox: MailboxId) -> Result<bool, MailError> {
        let result = sqlx::query("UPDATE messages SET mailbox_id = ? WHERE id = ?")
            .bind(mailbox.value())
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("set_message_mailbox", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Returns the ids of every message in `mailbox`.
    ///
    /// Used by Empty Trash to reclaim the out-of-database raw `.eml` bodies
    /// (keyed by message id) before the rows are deleted.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn list_ids_in_mailbox(
        &self,
        mailbox: MailboxId,
    ) -> Result<Vec<MessageId>, MailError> {
        let ids = sqlx::query_scalar::<_, i64>("SELECT id FROM messages WHERE mailbox_id = ?")
            .bind(mailbox.value())
            .fetch_all(self.pool)
            .await
            .map_err(|error| map_sqlx("list_ids_in_mailbox", error))?;
        Ok(ids.into_iter().map(MessageId::new).collect())
    }

    /// Deletes every message row in `mailbox`, returning how many were removed.
    /// Cascades (recipients, FTS content, tags, pending ops) run automatically.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete_in_mailbox(&self, mailbox: MailboxId) -> Result<u64, MailError> {
        let result = sqlx::query("DELETE FROM messages WHERE mailbox_id = ?")
            .bind(mailbox.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("delete_messages_in_mailbox", error))?;
        Ok(result.rows_affected())
    }

    /// Returns the ids of every message stored, across all accounts and folders.
    ///
    /// Used by Compact to detect orphaned raw `.eml` blobs on disk (a blob whose
    /// message id is absent from this set has no owning row and can be removed).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn all_ids(&self) -> Result<Vec<MessageId>, MailError> {
        let ids = sqlx::query_scalar::<_, i64>("SELECT id FROM messages")
            .fetch_all(self.pool)
            .await
            .map_err(|error| map_sqlx("all_message_ids", error))?;
        Ok(ids.into_iter().map(MessageId::new).collect())
    }
}
