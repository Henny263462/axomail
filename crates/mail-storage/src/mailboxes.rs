//! Mailbox repository: upsert folders discovered from a server and list
//! the folders of an account.

use axomail_core::{AccountId, MailError, Mailbox, MailboxId, MailboxName, MailboxRole};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;

/// Mailbox data to upsert, keyed by `(account, name)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewMailbox {
    /// Owning account.
    pub account: AccountId,
    /// Decoded folder name.
    pub name: MailboxName,
    /// Detected or server-declared role.
    pub role: MailboxRole,
    /// IMAP `UIDVALIDITY`, if known.
    pub uid_validity: Option<u32>,
    /// IMAP `UIDNEXT`, if known.
    pub uid_next: Option<u32>,
}

/// Repository over the `mailboxes` table.
#[derive(Debug)]
pub struct MailboxRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> MailboxRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Inserts a mailbox, or updates its role and remote state if one
    /// with the same `(account, name)` already exists. Returns the
    /// stored [`Mailbox`]; repeated calls are idempotent and preserve
    /// the identifier.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn upsert(&self, new: &NewMailbox) -> Result<Mailbox, MailError> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO mailboxes (account_id, name, role, uid_validity, uid_next)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (account_id, name) DO UPDATE SET
                 role = excluded.role,
                 uid_validity = excluded.uid_validity,
                 uid_next = excluded.uid_next
             RETURNING id",
        )
        .bind(new.account.value())
        .bind(new.name.as_str())
        .bind(role_str(new.role))
        .bind(new.uid_validity.map(i64::from))
        .bind(new.uid_next.map(i64::from))
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("upsert_mailbox", error))?;

        Ok(Mailbox {
            id: MailboxId::new(id),
            account: new.account,
            name: new.name.clone(),
            role: new.role,
        })
    }

    /// Fetches a single mailbox by id, or `None` if absent.
    ///
    /// Added for the message-detail command, which resolves a message's
    /// owning account from its mailbox id without loading the whole tree.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure and if a stored
    /// row cannot be decoded into a valid [`Mailbox`].
    pub async fn get(&self, id: MailboxId) -> Result<Option<Mailbox>, MailError> {
        let row = sqlx::query_as::<_, MailboxRow>(
            "SELECT id, account_id, name, role FROM mailboxes WHERE id = ?",
        )
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
        let rows = sqlx::query_as::<_, MailboxRow>(
            "SELECT id, account_id, name, role FROM mailboxes WHERE account_id = ? ORDER BY id",
        )
        .bind(account.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_mailboxes", error))?;

        rows.into_iter().map(MailboxRow::into_mailbox).collect()
    }
}

/// Raw `mailboxes` row, mapped by column name.
#[derive(sqlx::FromRow)]
struct MailboxRow {
    id: i64,
    account_id: i64,
    name: String,
    role: String,
}

impl MailboxRow {
    fn into_mailbox(self) -> Result<Mailbox, MailError> {
        Ok(Mailbox {
            id: MailboxId::new(self.id),
            account: AccountId::new(self.account_id),
            name: MailboxName::new(self.name),
            role: parse_role(&self.role)?,
        })
    }
}

/// Serialized form of a mailbox role.
fn role_str(role: MailboxRole) -> &'static str {
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
