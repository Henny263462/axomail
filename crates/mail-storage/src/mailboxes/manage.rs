//! Server-first folder CRUD for [`MailboxRepo`].
//!
//! These are the storage side of the folder commands: the caller runs the
//! server operation first and calls these only on success, so the local tree
//! can never grow a phantom folder the server does not have. Renames rewrite a
//! whole subtree's path prefixes in one transaction, preserving ids (and
//! messages) throughout.

use axomail_core::{AccountId, MailError, Mailbox, MailboxId, MailboxName, MailboxRole};
use sqlx::{Sqlite, SqliteConnection, Transaction};

use crate::error_map::map_sqlx;

use super::{MAILBOX_COLUMNS, MailboxRepo, MailboxRow, parent_path, role_str};

impl MailboxRepo<'_> {
    /// Inserts a folder the caller just created on the server. `remote_name` is
    /// the full server path; the display `name` mirrors it.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn create_local(
        &self,
        account: AccountId,
        parent: Option<MailboxId>,
        remote_name: &MailboxName,
        delimiter: Option<&str>,
        role: MailboxRole,
    ) -> Result<Mailbox, MailError> {
        let row = sqlx::query_as::<_, MailboxRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO mailboxes
                 (account_id, name, remote_name, role, parent_id, delimiter,
                  is_subscribed, is_orphaned, discovered_at)
             VALUES (?, ?, ?, ?, ?, ?, 1, 0, unixepoch())
             RETURNING {MAILBOX_COLUMNS}"
        )))
        .bind(account.value())
        .bind(remote_name.as_str())
        .bind(remote_name.as_str())
        .bind(role_str(role))
        .bind(parent.map(MailboxId::value))
        .bind(delimiter)
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("create_mailbox_local", error))?;
        row.into_mailbox()
    }

    /// Renames a folder and every descendant, preserving ids and messages.
    ///
    /// The folder's `remote_name`/`name` become `new_remote_name`, its
    /// `parent_id` is re-resolved from the new path, and each descendant's path
    /// prefix is rewritten from the old path to the new one.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn rename_subtree(
        &self,
        id: MailboxId,
        new_remote_name: &MailboxName,
    ) -> Result<(), MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("rename_begin", error))?;

        let Some(current) = load_one(&mut tx, id).await? else {
            return Err(MailError::Database {
                message: "rename_mailbox: the folder no longer exists".to_owned(),
            });
        };
        let account = current.account_id;
        let old_remote = current.remote_name;
        let delimiter = current.delimiter.clone();
        let new_remote = new_remote_name.as_str();

        // Re-resolve the parent from the new path (handles rename and move).
        let parent_id = match parent_path(new_remote, delimiter.as_deref()) {
            Some(parent) => resolve_id(&mut tx, account, &parent).await?,
            None => None,
        };
        rename_row(&mut tx, id, new_remote, parent_id).await?;

        // Rewrite descendants whose path starts with "<old>/…".
        if let Some(delimiter) = delimiter.as_deref() {
            let prefix = format!("{old_remote}{delimiter}");
            rewrite_descendants(&mut tx, account, &old_remote, &prefix, new_remote).await?;
        }

        tx.commit()
            .await
            .map_err(|error| map_sqlx("rename_commit", error))
    }

    /// Sets a folder's local subscription flag after the server op succeeded.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set_subscribed(&self, id: MailboxId, subscribed: bool) -> Result<(), MailError> {
        sqlx::query("UPDATE mailboxes SET is_subscribed = ? WHERE id = ?")
            .bind(i64::from(subscribed))
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("set_mailbox_subscribed", error))?;
        Ok(())
    }

    /// Deletes a folder locally after the server delete succeeded. Its messages
    /// cascade away; any child folders are re-parented to the root (FK
    /// `ON DELETE SET NULL`).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete(&self, id: MailboxId) -> Result<(), MailError> {
        sqlx::query("DELETE FROM mailboxes WHERE id = ?")
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("delete_mailbox", error))?;
        Ok(())
    }
}

/// A minimal row read for the rename path.
#[derive(sqlx::FromRow)]
struct OneRow {
    account_id: i64,
    remote_name: String,
    delimiter: Option<String>,
}

async fn load_one(
    tx: &mut Transaction<'_, Sqlite>,
    id: MailboxId,
) -> Result<Option<OneRow>, MailError> {
    sqlx::query_as::<_, OneRow>(
        "SELECT account_id, remote_name, delimiter FROM mailboxes WHERE id = ?",
    )
    .bind(id.value())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| map_sqlx("rename_load", error))
}

async fn resolve_id(
    tx: &mut Transaction<'_, Sqlite>,
    account: i64,
    remote_name: &str,
) -> Result<Option<i64>, MailError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT id FROM mailboxes WHERE account_id = ? AND remote_name = ?",
    )
    .bind(account)
    .bind(remote_name)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| map_sqlx("rename_resolve", error))
}

async fn rename_row(
    tx: &mut Transaction<'_, Sqlite>,
    id: MailboxId,
    new_remote: &str,
    parent_id: Option<i64>,
) -> Result<(), MailError> {
    sqlx::query("UPDATE mailboxes SET remote_name = ?, name = ?, parent_id = ? WHERE id = ?")
        .bind(new_remote)
        .bind(new_remote)
        .bind(parent_id)
        .bind(id.value())
        .execute(&mut **tx)
        .await
        .map_err(|error| map_sqlx("rename_row", error))?;
    Ok(())
}

/// Rewrites every descendant's path prefix from the old path to the new one.
///
/// A descendant's path begins `"<old_remote><delimiter>…"`; the leading
/// `old_remote` run (its byte length is `old_len`) is replaced by `new_remote`,
/// keeping the delimiter and tail. Matched by an exact prefix compare on the
/// same length, so no `LIKE` wildcard escaping is needed.
async fn rewrite_descendants(
    tx: &mut SqliteConnection,
    account: i64,
    old_remote: &str,
    prefix: &str,
    new_remote: &str,
) -> Result<(), MailError> {
    let old_len = i64::try_from(old_remote.len()).unwrap_or(i64::MAX);
    let prefix_len = i64::try_from(prefix.len()).unwrap_or(i64::MAX);
    sqlx::query(
        "UPDATE mailboxes
             SET remote_name = ?1 || substr(remote_name, ?2),
                 name = ?1 || substr(remote_name, ?2)
         WHERE account_id = ?3 AND substr(remote_name, 1, ?4) = ?5",
    )
    .bind(new_remote)
    .bind(old_len + 1)
    .bind(account)
    .bind(prefix_len)
    .bind(prefix)
    .execute(tx)
    .await
    .map_err(|error| map_sqlx("rename_descendants", error))?;
    Ok(())
}
