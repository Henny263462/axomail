//! Mailbox repository: the folder tree of an account.
//!
//! Beyond the simple seed [`MailboxRepo::upsert`] and reads, this repo owns
//! the discovery [`reconcile`](MailboxRepo::reconcile) (upsert by full server
//! path, rename detection, conservative orphan handling), the server-first
//! folder CRUD helpers, and the indexed per-folder unread/total
//! [`counts`](MailboxRepo::counts_for_account).

mod manage;
mod reconcile;

use std::collections::HashMap;

use axomail_core::{AccountId, MailError, Mailbox, MailboxId, MailboxName, MailboxRole};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;

pub use reconcile::{DiscoveredMailbox, ReconcileSummary};

/// The parent path of a full folder path under `delimiter`, or `None` when the
/// folder is top-level or flat. Shared by the reconcile and the rename path.
pub(crate) fn parent_path(remote: &str, delimiter: Option<&str>) -> Option<String> {
    let delimiter = delimiter?;
    if delimiter.is_empty() {
        return None;
    }
    remote.rfind(delimiter).map(|idx| remote[..idx].to_owned())
}

/// Mailbox data to insert as a local seed, keyed by `(account, name)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewMailbox {
    /// Owning account.
    pub account: AccountId,
    /// Decoded, unique display name.
    pub name: MailboxName,
    /// Full server path the folder is addressed by. Equals `name` for a flat
    /// seed; for the IMAP inbox seed it is the server's canonical `INBOX`.
    pub remote_name: MailboxName,
    /// Detected or server-declared role.
    pub role: MailboxRole,
    /// IMAP `UIDVALIDITY`, if known.
    pub uid_validity: Option<u32>,
    /// IMAP `UIDNEXT`, if known.
    pub uid_next: Option<u32>,
}

/// Per-folder message counts for the sidebar badges/tooltips.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxCounts {
    /// Total messages in the folder.
    pub total: i64,
    /// Unread (unseen) messages in the folder.
    pub unread: i64,
}

/// Columns selected when reading a mailbox, in [`MailboxRow`] order.
pub(crate) const MAILBOX_COLUMNS: &str =
    "id, account_id, name, role, parent_id, remote_name, delimiter, is_subscribed, is_orphaned";

/// Repository over the `mailboxes` table.
#[derive(Debug)]
pub struct MailboxRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> MailboxRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Inserts a seed mailbox, or updates its role and remote state if one
    /// with the same `(account, name)` already exists. Returns the stored
    /// [`Mailbox`]; repeated calls are idempotent and preserve the identifier.
    ///
    /// Seeds are always top-level, flat, subscribed, and non-orphaned. The
    /// discovery [`reconcile`](Self::reconcile) fills in real hierarchy later.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn upsert(&self, new: &NewMailbox) -> Result<Mailbox, MailError> {
        let row = sqlx::query_as::<_, MailboxRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO mailboxes
                 (account_id, name, remote_name, role, uid_validity, uid_next)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (account_id, name) DO UPDATE SET
                 role = excluded.role,
                 uid_validity = excluded.uid_validity,
                 uid_next = excluded.uid_next
             RETURNING {MAILBOX_COLUMNS}"
        )))
        .bind(new.account.value())
        .bind(new.name.as_str())
        .bind(new.remote_name.as_str())
        .bind(role_str(new.role))
        .bind(new.uid_validity.map(i64::from))
        .bind(new.uid_next.map(i64::from))
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("upsert_mailbox", error))?;

        row.into_mailbox()
    }

    /// Fetches a single mailbox by id, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure and if a stored
    /// row cannot be decoded into a valid [`Mailbox`].
    pub async fn get(&self, id: MailboxId) -> Result<Option<Mailbox>, MailError> {
        let row = sqlx::query_as::<_, MailboxRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {MAILBOX_COLUMNS} FROM mailboxes WHERE id = ?"
        )))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_mailbox", error))?;

        row.map(MailboxRow::into_mailbox).transpose()
    }

    /// Lists the mailboxes of an account ordered by identifier.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure and if a
    /// stored row cannot be decoded into a valid [`Mailbox`].
    pub async fn list(&self, account: AccountId) -> Result<Vec<Mailbox>, MailError> {
        let rows = sqlx::query_as::<_, MailboxRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {MAILBOX_COLUMNS} FROM mailboxes WHERE account_id = ? ORDER BY id"
        )))
        .bind(account.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_mailboxes", error))?;

        rows.into_iter().map(MailboxRow::into_mailbox).collect()
    }

    /// Returns per-folder `{ total, unread }` counts for every folder of the
    /// account that holds at least one message, in a single grouped query.
    ///
    /// Folders absent from the map hold no messages (both counts zero). The
    /// grouping walks `idx_messages_mailbox_date`'s `mailbox_id` prefix; the
    /// per-folder [`counts`](Self::counts) path additionally leans on the
    /// partial `idx_messages_unread` index for the unseen tally.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn counts_for_account(
        &self,
        account: AccountId,
    ) -> Result<HashMap<MailboxId, MailboxCounts>, MailError> {
        let rows = sqlx::query_as::<_, CountRow>(
            "SELECT m.mailbox_id AS mailbox_id,
                    COUNT(*) AS total,
                    COALESCE(SUM(CASE WHEN m.flag_seen = 0 THEN 1 ELSE 0 END), 0) AS unread
             FROM messages m
             JOIN mailboxes mb ON mb.id = m.mailbox_id
             WHERE mb.account_id = ?
             GROUP BY m.mailbox_id",
        )
        .bind(account.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("mailbox_counts_for_account", error))?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    MailboxId::new(row.mailbox_id),
                    MailboxCounts {
                        total: row.total,
                        unread: row.unread,
                    },
                )
            })
            .collect())
    }

    /// Returns the `{ total, unread }` counts for a single folder. The unread
    /// tally is served by the partial `idx_messages_unread` index.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn counts(&self, mailbox: MailboxId) -> Result<MailboxCounts, MailError> {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE mailbox_id = ?")
            .bind(mailbox.value())
            .fetch_one(self.pool)
            .await
            .map_err(|error| map_sqlx("mailbox_total_count", error))?;
        let unread: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE mailbox_id = ? AND flag_seen = 0",
        )
        .bind(mailbox.value())
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("mailbox_unread_count", error))?;
        Ok(MailboxCounts { total, unread })
    }
}

/// Raw `mailboxes` row, mapped by column name (see [`MAILBOX_COLUMNS`]).
#[derive(sqlx::FromRow)]
pub(crate) struct MailboxRow {
    id: i64,
    account_id: i64,
    name: String,
    role: String,
    parent_id: Option<i64>,
    remote_name: String,
    delimiter: Option<String>,
    is_subscribed: i64,
    is_orphaned: i64,
}

impl MailboxRow {
    pub(crate) fn into_mailbox(self) -> Result<Mailbox, MailError> {
        Ok(Mailbox {
            id: MailboxId::new(self.id),
            account: AccountId::new(self.account_id),
            name: MailboxName::new(self.name),
            role: parse_role(&self.role)?,
            parent: self.parent_id.map(MailboxId::new),
            remote_name: MailboxName::new(self.remote_name),
            delimiter: self.delimiter,
            subscribed: self.is_subscribed != 0,
            orphaned: self.is_orphaned != 0,
        })
    }
}

/// Grouped-count row for [`MailboxRepo::counts_for_account`].
#[derive(sqlx::FromRow)]
struct CountRow {
    mailbox_id: i64,
    total: i64,
    unread: i64,
}

/// Serialized form of a mailbox role.
pub(crate) fn role_str(role: MailboxRole) -> &'static str {
    match role {
        MailboxRole::Inbox => "inbox",
        MailboxRole::Drafts => "drafts",
        MailboxRole::Sent => "sent",
        MailboxRole::Trash => "trash",
        MailboxRole::Archive => "archive",
        MailboxRole::Junk => "junk",
        MailboxRole::Custom => "custom",
    }
}

/// Parses a stored mailbox-role string.
fn parse_role(value: &str) -> Result<MailboxRole, MailError> {
    let role = match value {
        "inbox" => MailboxRole::Inbox,
        "drafts" => MailboxRole::Drafts,
        "sent" => MailboxRole::Sent,
        "trash" => MailboxRole::Trash,
        "archive" => MailboxRole::Archive,
        "junk" => MailboxRole::Junk,
        "custom" => MailboxRole::Custom,
        _ => {
            return Err(MailError::Database {
                message: "read_mailbox: unknown mailbox role".to_owned(),
            });
        }
    };
    Ok(role)
}
