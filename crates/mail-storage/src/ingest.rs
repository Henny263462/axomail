//! Transactional ingestion of a [`SyncOutcome`] into local storage.
//!
//! One [`IngestRepo::apply`] call persists a single sync pass atomically:
//! new envelopes and their senders/recipients are inserted, flag changes
//! and removals are applied by remote id, and the mailbox's sync state is
//! advanced — all in one short transaction so a crash leaves the store on
//! a clean pass boundary. When the server reports a new `UIDVALIDITY`
//! every cached UID is meaningless, so the mailbox's messages are wiped
//! before the fresh envelopes land.

use axomail_core::{
    Address, MailError, MailboxId, MailboxSyncState, MessageFlags, RemoteEnvelope, RemoteMessageId,
    SyncOutcome,
};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use crate::error_map::map_sqlx;
use crate::message_rows::RemoteColumns;
use crate::sync_state::{read_state, write_state};

/// Actual row-level effect of applying one [`SyncOutcome`].
///
/// Counts reflect rows genuinely changed, which can be lower than the
/// outcome's list lengths: an envelope already present is not
/// re-inserted, and a flag update or removal for an unknown message is a
/// no-op. This is what the engine reports as real progress.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestSummary {
    /// New messages actually inserted.
    pub inserted: u64,
    /// Messages whose flags actually changed.
    pub flags_updated: u64,
    /// Messages actually removed.
    pub removed: u64,
    /// Whether a `UIDVALIDITY` change wiped the mailbox first.
    pub wiped: bool,
}

/// Repository that applies sync outcomes to the message store.
#[derive(Debug)]
pub struct IngestRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> IngestRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Applies one sync pass for `mailbox` in a single transaction and
    /// reports the rows actually affected.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if an
    /// envelope carries a size that cannot be stored. The transaction is
    /// rolled back on error, leaving the mailbox unchanged.
    pub async fn apply(
        &self,
        mailbox: MailboxId,
        outcome: &SyncOutcome,
    ) -> Result<IngestSummary, MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("ingest_begin", error))?;

        let stored = read_state(&mut *tx, mailbox).await?;
        let wiped = uid_validity_changed(stored, outcome.uid_validity);
        if wiped {
            sqlx::query("DELETE FROM messages WHERE mailbox_id = ?")
                .bind(mailbox.value())
                .execute(&mut *tx)
                .await
                .map_err(|error| map_sqlx("ingest_wipe_mailbox", error))?;
        }

        let mut inserted = 0u64;
        for envelope in &outcome.new_envelopes {
            if let Some(message_id) = insert_envelope(&mut tx, mailbox, envelope).await? {
                insert_recipients(&mut tx, message_id, "from", &envelope.from).await?;
                insert_recipients(&mut tx, message_id, "to", &envelope.to).await?;
                index_envelope_content(&mut tx, message_id, envelope).await?;
                inserted += 1;
            }
        }

        let mut flags_updated = 0u64;
        for (remote_id, flags) in &outcome.flag_updates {
            flags_updated += apply_flags(&mut tx, mailbox, remote_id, *flags).await?;
        }

        let mut removed = 0u64;
        for remote_id in &outcome.removed {
            removed += delete_message(&mut tx, mailbox, remote_id).await?;
        }

        let next_state = MailboxSyncState {
            uid_validity: outcome.uid_validity,
            highest_uid: outcome.highest_uid,
        };
        write_state(&mut *tx, mailbox, &next_state).await?;

        tx.commit()
            .await
            .map_err(|error| map_sqlx("ingest_commit", error))?;

        Ok(IngestSummary {
            inserted,
            flags_updated,
            removed,
            wiped,
        })
    }
}

/// A wipe is required only when a previously recorded `UIDVALIDITY`
/// differs from the one reported now; a first sync (no stored validity)
/// or a protocol without validity never wipes.
fn uid_validity_changed(stored: MailboxSyncState, reported: Option<u32>) -> bool {
    matches!(
        (stored.uid_validity, reported),
        (Some(old), Some(new)) if old != new
    )
}

/// Inserts one envelope, skipping it if its remote identity already
/// exists (idempotent re-application after a crash). Returns the new
/// message id, or `None` when the row was already present.
async fn insert_envelope(
    conn: &mut SqliteConnection,
    mailbox: MailboxId,
    envelope: &RemoteEnvelope,
) -> Result<Option<i64>, MailError> {
    let remote = RemoteColumns::from_remote_id(Some(&envelope.remote_id));
    let date = envelope.date.map(|date| date.unix_seconds());
    let size = envelope
        .size_bytes
        .map(i64::try_from)
        .transpose()
        .map_err(|_| MailError::Database {
            message: "ingest_message: size out of range".to_owned(),
        })?;

    // `ON CONFLICT DO NOTHING` covers the IMAP unique index on
    // (mailbox_id, remote_uid_validity, remote_uid); a skipped row yields
    // no `RETURNING` output, so recipients are only written for genuine
    // inserts.
    let id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO messages (
             mailbox_id, remote_kind, remote_uid, remote_uid_validity, remote_uidl,
             subject, date_unix,
             flag_seen, flag_answered, flag_flagged, flag_draft, flag_deleted,
             size_bytes, preview, has_raw
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 0)
         ON CONFLICT DO NOTHING
         RETURNING id",
    )
    .bind(mailbox.value())
    .bind(remote.kind)
    .bind(remote.uid)
    .bind(remote.uid_validity)
    .bind(remote.uidl.as_deref())
    .bind(envelope.subject.as_deref())
    .bind(date)
    .bind(envelope.flags.seen)
    .bind(envelope.flags.answered)
    .bind(envelope.flags.flagged)
    .bind(envelope.flags.draft)
    .bind(envelope.flags.deleted)
    .bind(size)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|error| map_sqlx("ingest_message", error))?;

    Ok(id)
}

/// Indexes a freshly inserted envelope's header text (subject and
/// sender/recipient names and addresses) so it is searchable immediately.
/// Envelopes carry no body text yet; the message-detail path fills the
/// body in later via [`crate::SearchRepo::index_message_body`].
async fn index_envelope_content(
    conn: &mut SqliteConnection,
    message_id: i64,
    envelope: &RemoteEnvelope,
) -> Result<(), MailError> {
    let correspondents =
        crate::search::correspondents_text(envelope.from.iter().chain(&envelope.to));
    crate::search::upsert_content(
        conn,
        message_id,
        envelope.subject.as_deref(),
        &correspondents,
        "",
    )
    .await
}

/// Inserts one kind of recipient list, preserving order via `position`.
async fn insert_recipients(
    conn: &mut SqliteConnection,
    message_id: i64,
    kind: &str,
    addresses: &[Address],
) -> Result<(), MailError> {
    for (position, address) in addresses.iter().enumerate() {
        sqlx::query(
            "INSERT INTO message_recipients (message_id, kind, display_name, email, position)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(message_id)
        .bind(kind)
        .bind(address.display_name.as_deref())
        .bind(address.email.as_str())
        .bind(i64::try_from(position).unwrap_or(i64::MAX))
        .execute(&mut *conn)
        .await
        .map_err(|error| map_sqlx("ingest_recipient", error))?;
    }
    Ok(())
}

/// Overwrites the flags of the message with a matching remote identity in
/// this mailbox. Returns the number of rows changed (`0` if unknown).
async fn apply_flags(
    conn: &mut SqliteConnection,
    mailbox: MailboxId,
    remote_id: &RemoteMessageId,
    flags: MessageFlags,
) -> Result<u64, MailError> {
    let mut builder = sqlx::query(sqlx::AssertSqlSafe(flags_sql(remote_id)))
        .bind(flags.seen)
        .bind(flags.answered)
        .bind(flags.flagged)
        .bind(flags.draft)
        .bind(flags.deleted)
        .bind(mailbox.value());
    builder = bind_remote(builder, remote_id);
    let result = builder
        .execute(&mut *conn)
        .await
        .map_err(|error| map_sqlx("ingest_flags", error))?;
    Ok(result.rows_affected())
}

/// Deletes the message with a matching remote identity in this mailbox.
/// The `WHERE` clause intersects with stored rows, so `rows_affected`
/// reports how many were genuinely present and removed.
async fn delete_message(
    conn: &mut SqliteConnection,
    mailbox: MailboxId,
    remote_id: &RemoteMessageId,
) -> Result<u64, MailError> {
    let sql = format!(
        "DELETE FROM messages WHERE mailbox_id = ? AND {}",
        remote_predicate(remote_id)
    );
    let mut builder = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(mailbox.value());
    builder = bind_remote(builder, remote_id);
    let result = builder
        .execute(&mut *conn)
        .await
        .map_err(|error| map_sqlx("ingest_delete", error))?;
    Ok(result.rows_affected())
}

/// `UPDATE ... SET flags WHERE mailbox and remote identity` statement.
fn flags_sql(remote_id: &RemoteMessageId) -> String {
    format!(
        "UPDATE messages SET
             flag_seen = ?, flag_answered = ?, flag_flagged = ?,
             flag_draft = ?, flag_deleted = ?
         WHERE mailbox_id = ? AND {}",
        remote_predicate(remote_id)
    )
}

/// SQL predicate matching a message by its remote identity. The bound
/// values are supplied by [`bind_remote`] in the same order.
fn remote_predicate(remote_id: &RemoteMessageId) -> &'static str {
    match remote_id {
        RemoteMessageId::Imap { .. } => {
            "remote_kind = 'imap' AND remote_uid_validity = ? AND remote_uid = ?"
        }
        RemoteMessageId::Pop3 { .. } => "remote_kind = 'pop3' AND remote_uidl = ?",
    }
}

/// Binds the identity values for [`remote_predicate`] in matching order.
fn bind_remote<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>,
    remote_id: &'q RemoteMessageId,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments> {
    match remote_id {
        RemoteMessageId::Imap { uid, uid_validity } => {
            query.bind(i64::from(*uid_validity)).bind(i64::from(*uid))
        }
        RemoteMessageId::Pop3 { uidl } => query.bind(uidl.as_str()),
    }
}
