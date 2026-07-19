//! System-flag and keyword application for one sync pass.
//!
//! Split out of the ingest apply loop: overwrites a message's five system-flag
//! columns and reconciles its keyword tags to the server-reported set — while
//! **skipping** any message whose optimistic local change still has an
//! un-replayed write-back op ([`PendingOpGuards`]). Without that guard, a sync
//! pass would clobber a just-set star or tag back to the server's stale value
//! before its `store_flags` reached the server; if the op were then parked, the
//! change would be lost for good.

use std::collections::HashSet;

use axomail_core::{AccountId, MailError, MailboxId, MessageFlags, RemoteMessageId};
use sqlx::SqliteConnection;

use crate::error_map::map_sqlx;

use super::{bind_remote, remote_predicate};

/// Overwrites the system flags of the message with a matching remote identity in
/// this mailbox, and reconciles its keyword tags to the server-reported set.
/// Returns the number of rows changed (`0` if unknown or guarded).
///
/// The five system-flag columns are written by remote identity; the keyword set
/// is reconciled against `message_tags` (auto-creating account tags for unknown
/// keywords) so a keyword the server dropped is unlinked locally.
///
/// A message with an un-replayed write-back op is **skipped** for the matching
/// overwrite (`guards`): applying the server's stale value would clobber the
/// user's optimistic local change (a star, a tag) before its `store_flags` has
/// reached the server — and if that op is later parked, the change would be lost
/// for good. The two overwrites are guarded independently, so a queued keyword
/// op never freezes a legitimate system-flag update and vice versa. Once the op
/// replays and is removed from `pending_ops`, the guard lifts and the server's
/// state applies normally. The server-side write path (`store_flags`) is
/// delta-based, so nothing is clobbered on the server.
pub(super) async fn apply_flags(
    conn: &mut SqliteConnection,
    mailbox: MailboxId,
    account: AccountId,
    remote_id: &RemoteMessageId,
    flags: &MessageFlags,
    guards: &PendingOpGuards,
) -> Result<u64, MailError> {
    // Resolve the local row first so the write-back guards can be consulted.
    let Some(message_id) = resolve_message_id(conn, mailbox, remote_id).await? else {
        return Ok(0);
    };

    let mut updated = 0;
    if !guards.flags.contains(&message_id) {
        let mut builder = sqlx::query(sqlx::AssertSqlSafe(flags_sql(remote_id)))
            .bind(flags.seen)
            .bind(flags.answered)
            .bind(flags.flagged)
            .bind(flags.draft)
            .bind(flags.deleted)
            .bind(mailbox.value());
        builder = bind_remote(builder, remote_id);
        updated = builder
            .execute(&mut *conn)
            .await
            .map_err(|error| map_sqlx("ingest_flags", error))?
            .rows_affected();
    }

    if !guards.keywords.contains(&message_id) {
        crate::tags::sync::reconcile_message_keywords(conn, account, message_id, &flags.keywords)
            .await?;
    }
    Ok(updated)
}

/// Message ids (within one account) whose optimistic local state has an
/// un-replayed write-back op, split by which server overwrite each op guards.
#[derive(Default)]
pub(super) struct PendingOpGuards {
    /// Messages with a queued system-flag op (`add_flags` / `remove_flags`).
    flags: HashSet<i64>,
    /// Messages with a queued keyword op (`add_keyword` / `remove_keyword`).
    keywords: HashSet<i64>,
}

/// Loads the [`PendingOpGuards`] for `account` on the ingest transaction's
/// connection. Reads the very table the write-back queue drains: a completed op
/// is deleted, so any surviving row means the local change has not yet reached
/// the server and must not be overwritten by this pass.
pub(super) async fn load_pending_op_guards(
    conn: &mut SqliteConnection,
    account: AccountId,
) -> Result<PendingOpGuards, MailError> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT message_id, kind FROM pending_ops
         WHERE account_id = ?
           AND kind IN ('add_flags', 'remove_flags', 'add_keyword', 'remove_keyword')",
    )
    .bind(account.value())
    .fetch_all(&mut *conn)
    .await
    .map_err(|error| map_sqlx("ingest_pending_op_guards", error))?;

    let mut guards = PendingOpGuards::default();
    for (message_id, kind) in rows {
        match kind.as_str() {
            "add_flags" | "remove_flags" => {
                guards.flags.insert(message_id);
            }
            "add_keyword" | "remove_keyword" => {
                guards.keywords.insert(message_id);
            }
            _ => {}
        }
    }
    Ok(guards)
}

/// Resolves the local message id for a remote identity within a mailbox.
async fn resolve_message_id(
    conn: &mut SqliteConnection,
    mailbox: MailboxId,
    remote_id: &RemoteMessageId,
) -> Result<Option<i64>, MailError> {
    let id = match remote_id {
        RemoteMessageId::Imap { uid, uid_validity } => {
            sqlx::query_scalar::<_, i64>(
                "SELECT id FROM messages WHERE mailbox_id = ? AND remote_kind = 'imap' \
             AND remote_uid_validity = ? AND remote_uid = ?",
            )
            .bind(mailbox.value())
            .bind(i64::from(*uid_validity))
            .bind(i64::from(*uid))
            .fetch_optional(&mut *conn)
            .await
        }
        RemoteMessageId::Pop3 { uidl } => {
            sqlx::query_scalar::<_, i64>(
                "SELECT id FROM messages WHERE mailbox_id = ? AND remote_kind = 'pop3' \
             AND remote_uidl = ?",
            )
            .bind(mailbox.value())
            .bind(uidl.as_str())
            .fetch_optional(&mut *conn)
            .await
        }
    };
    id.map_err(|error| map_sqlx("ingest_resolve_message", error))
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
