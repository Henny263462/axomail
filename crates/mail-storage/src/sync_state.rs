//! Per-mailbox IMAP synchronization state, stored in the
//! `imap_sync_state` table.
//!
//! The table keeps one row per mailbox mapping directly to
//! [`axomail_core::MailboxSyncState`]: `uid_validity`, `last_seen_uid`
//! (the highest ingested UID), and `highest_modseq` (the CONDSTORE/QRESYNC
//! mod-sequence cursor). Absence of a row is treated as the "never synced"
//! default so a first sync starts cleanly.

use axomail_core::{MailError, MailboxId, MailboxSyncState};
use sqlx::{Executor, Sqlite, SqlitePool};

use crate::error_map::map_sqlx;

/// Repository over the `imap_sync_state` table.
#[derive(Debug)]
pub struct SyncStateRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> SyncStateRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Reads the stored state for `mailbox`, returning
    /// [`MailboxSyncState::default`] when no row exists yet (a first
    /// sync).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if a
    /// stored UID cannot be represented as a `u32`.
    pub async fn get(&self, mailbox: MailboxId) -> Result<MailboxSyncState, MailError> {
        read_state(self.pool, mailbox).await
    }

    /// Inserts or overwrites the stored state for `mailbox` and stamps
    /// the sync time.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set(&self, mailbox: MailboxId, state: &MailboxSyncState) -> Result<(), MailError> {
        write_state(self.pool, mailbox, state).await
    }
}

/// Reads the sync state for a mailbox against any executor (pool or an
/// open transaction), defaulting to the "never synced" state.
pub(crate) async fn read_state<'e, E>(
    executor: E,
    mailbox: MailboxId,
) -> Result<MailboxSyncState, MailError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row: Option<(Option<i64>, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT uid_validity, last_seen_uid, highest_modseq \
         FROM imap_sync_state WHERE mailbox_id = ?",
    )
    .bind(mailbox.value())
    .fetch_optional(executor)
    .await
    .map_err(|error| map_sqlx("read_sync_state", error))?;

    match row {
        None => Ok(MailboxSyncState::default()),
        Some((uid_validity, last_seen_uid, highest_modseq)) => Ok(MailboxSyncState {
            uid_validity: to_u32(uid_validity)?,
            highest_uid: to_u32(last_seen_uid)?,
            highest_modseq: to_u64(highest_modseq)?,
        }),
    }
}

/// Upserts the sync state for a mailbox against any executor, stamping
/// `last_sync_unix` with the current time.
pub(crate) async fn write_state<'e, E>(
    executor: E,
    mailbox: MailboxId,
    state: &MailboxSyncState,
) -> Result<(), MailError>
where
    E: Executor<'e, Database = Sqlite>,
{
    // A mod-sequence is a 63-bit value (RFC 7162), so it fits in the signed
    // INTEGER column; an out-of-range value is stored as NULL rather than
    // corrupting the cursor, which safely degrades to a full flag/present-set
    // resync on the next pass.
    let modseq = state
        .highest_modseq
        .and_then(|value| i64::try_from(value).ok());
    sqlx::query(
        "INSERT INTO imap_sync_state \
             (mailbox_id, uid_validity, last_seen_uid, highest_modseq, last_sync_unix)
         VALUES (?, ?, ?, ?, unixepoch())
         ON CONFLICT (mailbox_id) DO UPDATE SET
             uid_validity = excluded.uid_validity,
             last_seen_uid = excluded.last_seen_uid,
             highest_modseq = excluded.highest_modseq,
             last_sync_unix = excluded.last_sync_unix",
    )
    .bind(mailbox.value())
    .bind(state.uid_validity.map(i64::from))
    .bind(state.highest_uid.map(i64::from))
    .bind(modseq)
    .execute(executor)
    .await
    .map_err(|error| map_sqlx("write_sync_state", error))?;
    Ok(())
}

/// Converts a stored, nullable UID column into an optional `u32`.
fn to_u32(value: Option<i64>) -> Result<Option<u32>, MailError> {
    value
        .map(u32::try_from)
        .transpose()
        .map_err(|_| MailError::Database {
            message: "read_sync_state: stored UID out of range".to_owned(),
        })
}

/// Converts a stored, nullable mod-sequence column into an optional `u64`.
fn to_u64(value: Option<i64>) -> Result<Option<u64>, MailError> {
    value
        .map(u64::try_from)
        .transpose()
        .map_err(|_| MailError::Database {
            message: "read_sync_state: stored HIGHESTMODSEQ out of range".to_owned(),
        })
}
