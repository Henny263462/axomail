//! Message relocation and bulk disposal for [`MessageRepo`].
//!
//! These are the storage side of the Wave 3 message actions and the Trash
//! lifecycle: the optimistic local move ([`set_mailbox`](MessageRepo::set_mailbox)),
//! the post-replay staleness marker ([`mark_remote_stale`](MessageRepo::mark_remote_stale)),
//! and the purgeable-only bulk deletion behind Empty Trash
//! ([`delete_in_mailbox_purgeable`](MessageRepo::delete_in_mailbox_purgeable))
//! plus Compact ([`all_ids`](MessageRepo::all_ids), which the caller diffs
//! against the raw `.eml` blobs on disk). Row deletes cascade to `message_recipients`,
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

/// Sentinel written into an IMAP row's (otherwise POP3-only) `remote_uidl`
/// column to flag its remote identity as STALE after a `move`/`delete` op
/// replayed to the server. A genuine IMAP row never carries a `remote_uidl`
/// (that column is POP3-only), so any IMAP row with a non-NULL `remote_uidl` is
/// unambiguously stale — which is exactly what the pending-ops claim excludes.
/// The leading control byte cannot collide with a real (printable, RFC 1939)
/// POP3 UIDL. See [`MessageRepo::mark_remote_stale`].
pub(crate) const STALE_UIDL_MARKER: &str = "\u{1}stale-after-move";

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

    /// Flags an IMAP message's remote identity STALE after its `move`/`delete`
    /// op replayed to the server. Returns whether a row was updated.
    ///
    /// A server move gives the message a **new UID** in the destination, so its
    /// stored UID+UIDVALIDITY no longer resolves there. Left as-is, the next
    /// per-message op (an undo move, a star, a mark-read) `SELECT`s the
    /// destination and the adapter's `resolve_uid` hits a UIDVALIDITY mismatch,
    /// returning a permanent [`MailError::Protocol`] that parks the op and
    /// diverges local from server.
    ///
    /// Rather than *clear* the identity — which would strand a local-only
    /// phantom that present-set reconciliation (scoped to `remote_kind = 'imap'`
    /// rows) can never heal, so the re-synced server copy silently duplicates it
    /// — this keeps `remote_kind`/`remote_uid`/`remote_uid_validity` intact and
    /// only writes [`STALE_UIDL_MARKER`] into the row's `remote_uidl`. Then:
    ///
    /// - the write-back queue skips ops for a stale-marked message (see the
    ///   `pending_ops` claim's stale-identity exclusion), so no later op ever
    ///   replays against the stale UID and parks; and
    /// - present-set reconciliation still sees the row (it kept its old UID) and
    ///   deletes it on the next full sync — the server copy is re-ingested fresh
    ///   and the stale op cascades away — so the row self-heals, no duplicate.
    ///
    /// Guarded to `remote_kind = 'imap'` so a genuine POP3 UIDL is never
    /// clobbered (and POP3 has no server move anyway).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn mark_remote_stale(&self, id: MessageId) -> Result<bool, MailError> {
        let result = sqlx::query(
            "UPDATE messages SET remote_uidl = ?
             WHERE id = ? AND remote_kind = 'imap'",
        )
        .bind(STALE_UIDL_MARKER)
        .bind(id.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("mark_message_remote_stale", error))?;
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

    /// Deletes the **purgeable** message rows in `mailbox` — those with no
    /// un-replayed relocate op still queued — and returns the ids actually
    /// deleted (cascades run automatically).
    ///
    /// A message in Trash can carry a still-pending `move`/`delete` pending op
    /// (it was moved/deleted to Trash *optimistically*; on the server it still
    /// sits in its source folder, not Trash). Empty Trash must **not** delete
    /// such a row: the row-delete cascades away the pending op
    /// (`pending_ops.message_id ON DELETE CASCADE`), stranding the message in
    /// its server source with no op left to relocate it — the mail resurrects.
    /// So rows with an outstanding `move`/`delete` op are excluded and survive
    /// to be purged by a later Empty Trash once their op has replayed (at which
    /// point the op row is gone and the message physically sits in Trash).
    /// A completed op leaves no row, so a fully-replayed Trash message *is*
    /// purged. Only relocate (`move`/`delete`) ops matter; a stray flag/keyword
    /// op never keeps a Trash message from being emptied.
    ///
    /// Returning the deleted ids (rather than a pre-snapshot) lets the caller
    /// reclaim exactly the raw `.eml` blobs whose rows are gone, so a message
    /// that left Trash between the caller's earlier read and this delete keeps
    /// its blob.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete_in_mailbox_purgeable(
        &self,
        mailbox: MailboxId,
    ) -> Result<Vec<MessageId>, MailError> {
        let ids = sqlx::query_scalar::<_, i64>(
            "DELETE FROM messages
              WHERE mailbox_id = ?
                AND NOT EXISTS (
                    SELECT 1 FROM pending_ops p
                     WHERE p.message_id = messages.id
                       AND p.kind IN ('move', 'delete')
                )
             RETURNING id",
        )
        .bind(mailbox.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("delete_purgeable_in_mailbox", error))?;
        Ok(ids.into_iter().map(MessageId::new).collect())
    }

    /// Whether a message row with `id` currently exists.
    ///
    /// Used by Compact to re-confirm, immediately before unlinking, that a raw
    /// `.eml` blob whose id was absent from the initial snapshot is *still*
    /// orphaned — closing the TOCTOU where a message inserted and body-downloaded
    /// after the snapshot would otherwise have its live blob deleted.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn exists(&self, id: MessageId) -> Result<bool, MailError> {
        let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM messages WHERE id = ?")
            .bind(id.value())
            .fetch_optional(self.pool)
            .await
            .map_err(|error| map_sqlx("message_exists", error))?;
        Ok(found.is_some())
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
