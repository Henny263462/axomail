//! Discovery reconcile and server-first folder CRUD for [`MailboxRepo`].
//!
//! [`reconcile`](super::MailboxRepo::reconcile) folds a freshly discovered
//! folder list into the local tree without ever losing local messages:
//!
//! * folders keyed by their full server path are **upserted** in place, so a
//!   folder that survives keeps its id (and its messages);
//! * a **moved subtree** — exactly one vanished folder and one new folder that
//!   share the same *leaf* name — is treated as the same folder relocated: the
//!   existing row's id (and messages) is preserved and its path updated. Only a
//!   preserved leaf positively confirms identity from names alone. A folder
//!   whose *leaf* changed cannot be told apart from an unrelated delete + create
//!   (e.g. deleting `Archive2024` and creating `Projects` under one parent), so
//!   it is **never** merged — merging would reassign the deleted folder's id,
//!   messages, and stale sync state to the unrelated new folder, and the next
//!   sync's `UIDVALIDITY` mismatch would then wipe the mis-attached cache. When
//!   identity cannot be positively confirmed, the safe path is taken (orphan the
//!   vanished folder, create the new one);
//! * a previously-discovered folder that vanished is an **orphan**: if it still
//!   holds local messages it is *flagged* (`is_orphaned`), never dropped
//!   (data-loss priority); only an empty orphan is deleted. A folder that was
//!   never on the server (`discovered_at IS NULL`, e.g. a local seed) is never
//!   an orphan candidate.
//!
//! The server-first folder CRUD lives in the sibling `manage` module.

use std::collections::{HashMap, HashSet};

use axomail_core::{AccountId, MailError, MailboxName, MailboxRole};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, Transaction};

use crate::error_map::map_sqlx;

use super::{MailboxRepo, parent_path, role_str};

/// A folder as discovered on the server, with its role already resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredMailbox {
    /// Full decoded server path (e.g. `INBOX/Work`).
    pub remote_name: MailboxName,
    /// Resolved role (SPECIAL-USE hint, else a name guess).
    pub role: MailboxRole,
    /// Server hierarchy delimiter, or `None` when flat.
    pub delimiter: Option<String>,
    /// Whether the folder should be treated as subscribed.
    pub subscribed: bool,
}

/// What one [`reconcile`](super::MailboxRepo::reconcile) changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconcileSummary {
    /// New folders inserted.
    pub created: u64,
    /// Existing folders whose attributes were refreshed.
    pub updated: u64,
    /// Folders whose full path changed but whose id (and messages) were kept.
    pub renamed: u64,
    /// Vanished folders kept and flagged because they hold local messages.
    pub orphaned: u64,
    /// Empty vanished folders removed.
    pub deleted: u64,
}

/// One existing row loaded for the reconcile diff.
#[derive(sqlx::FromRow)]
struct ExistingRow {
    id: i64,
    remote_name: String,
    discovered_at: Option<i64>,
    delimiter: Option<String>,
}

impl MailboxRepo<'_> {
    /// Reconciles the discovered folder list into the account's local tree.
    ///
    /// Runs in one transaction so a crash leaves the tree on a clean boundary.
    /// See the module docs for the upsert / rename / orphan rules.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn reconcile(
        &self,
        account: AccountId,
        discovered: &[DiscoveredMailbox],
    ) -> Result<ReconcileSummary, MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("reconcile_begin", error))?;

        let mut summary = ReconcileSummary::default();
        let discovered_names: HashSet<&str> = discovered
            .iter()
            .map(|folder| folder.remote_name.as_str())
            .collect();

        // Orphan candidates: previously-discovered folders now absent.
        let existing = load_existing(&mut tx, account).await?;
        let orphans: Vec<&ExistingRow> = existing
            .iter()
            .filter(|row| {
                row.discovered_at.is_some() && !discovered_names.contains(row.remote_name.as_str())
            })
            .collect();
        let existing_names: HashSet<&str> = existing
            .iter()
            .map(|row| row.remote_name.as_str())
            .collect();
        let new_folders: Vec<&DiscoveredMailbox> = discovered
            .iter()
            .filter(|folder| !existing_names.contains(folder.remote_name.as_str()))
            .collect();

        // Moved subtree: exactly one vanished folder and one new folder sharing
        // the same *leaf* name. A relocated folder keeps its leaf, so an equal
        // leaf positively identifies the pair as the same folder; preserve its
        // id (and messages). A leaf *change* is indistinguishable from an
        // unrelated delete + create, so it is deliberately not matched here and
        // falls through to the safe orphan/create path below.
        let mut renamed_remotes: HashSet<String> = HashSet::new();
        if let (Some(orphan), Some(new_folder)) = (single(&orphans), single(&new_folders))
            && leaf_name(&orphan.remote_name, orphan.delimiter.as_deref())
                == leaf_name(
                    new_folder.remote_name.as_str(),
                    new_folder.delimiter.as_deref(),
                )
        {
            rename_identity(&mut tx, orphan.id, new_folder.remote_name.as_str()).await?;
            renamed_remotes.insert(new_folder.remote_name.as_str().to_owned());
            summary.renamed += 1;
        }

        // Re-load after any rename so the upsert loop sees current identities.
        let fresh = load_existing(&mut tx, account).await?;
        let mut by_remote: HashMap<String, i64> = fresh
            .iter()
            .map(|row| (row.remote_name.clone(), row.id))
            .collect();

        // Parents before children so parent_id resolves against known rows.
        let mut ordered: Vec<&DiscoveredMailbox> = discovered.iter().collect();
        ordered.sort_by_key(|folder| {
            segment_count(folder.remote_name.as_str(), folder.delimiter.as_deref())
        });

        for folder in ordered {
            let remote = folder.remote_name.as_str();
            let parent_id = parent_path(remote, folder.delimiter.as_deref())
                .and_then(|parent| by_remote.get(&parent).copied());
            if let Some(&id) = by_remote.get(remote) {
                update_discovered(&mut tx, id, folder, parent_id).await?;
                if !renamed_remotes.contains(remote) {
                    summary.updated += 1;
                }
            } else {
                let id = insert_discovered(&mut tx, account, folder, parent_id).await?;
                by_remote.insert(remote.to_owned(), id);
                summary.created += 1;
            }
        }

        // Orphans not resolved by a rename: flag if they hold messages, else drop.
        for orphan in &orphans {
            if renamed_remotes_contains_id(&renamed_remotes, &by_remote, orphan.id) {
                continue;
            }
            if message_count(&mut tx, orphan.id).await? > 0 {
                flag_orphan(&mut tx, orphan.id).await?;
                summary.orphaned += 1;
            } else {
                delete_row(&mut tx, orphan.id).await?;
                summary.deleted += 1;
            }
        }

        tx.commit()
            .await
            .map_err(|error| map_sqlx("reconcile_commit", error))?;
        Ok(summary)
    }
}

/// Number of path segments (for parents-before-children ordering).
fn segment_count(remote: &str, delimiter: Option<&str>) -> usize {
    match delimiter {
        Some(d) if !d.is_empty() => remote.split(d).count(),
        _ => 1,
    }
}

/// The final path segment (leaf) of a folder path — the part after the last
/// delimiter, or the whole path when flat/top-level. The complement of
/// [`parent_path`]; used to positively identify a moved subtree by its
/// preserved leaf name.
fn leaf_name<'a>(remote: &'a str, delimiter: Option<&str>) -> &'a str {
    match delimiter {
        Some(d) if !d.is_empty() => remote.rsplit(d).next().unwrap_or(remote),
        _ => remote,
    }
}

/// Returns the single element of a slice, or `None` if it has zero or many.
fn single<T: Copy>(items: &[T]) -> Option<T> {
    match items {
        [only] => Some(*only),
        _ => None,
    }
}

/// True when `id` is the row that a rename just claimed `new_remote` for.
fn renamed_remotes_contains_id(
    renamed_remotes: &HashSet<String>,
    by_remote: &HashMap<String, i64>,
    id: i64,
) -> bool {
    renamed_remotes
        .iter()
        .any(|remote| by_remote.get(remote) == Some(&id))
}

async fn load_existing(
    tx: &mut Transaction<'_, Sqlite>,
    account: AccountId,
) -> Result<Vec<ExistingRow>, MailError> {
    sqlx::query_as::<_, ExistingRow>(
        "SELECT id, remote_name, discovered_at, delimiter FROM mailboxes WHERE account_id = ?",
    )
    .bind(account.value())
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| map_sqlx("reconcile_load", error))
}

/// Updates identity only (used by the rename-detection step).
async fn rename_identity(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
    new_remote: &str,
) -> Result<(), MailError> {
    sqlx::query("UPDATE mailboxes SET remote_name = ?, name = ?, is_orphaned = 0 WHERE id = ?")
        .bind(new_remote)
        .bind(new_remote)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|error| map_sqlx("reconcile_rename", error))?;
    Ok(())
}

async fn update_discovered(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
    folder: &DiscoveredMailbox,
    parent_id: Option<i64>,
) -> Result<(), MailError> {
    sqlx::query(
        "UPDATE mailboxes
             SET role = ?, delimiter = ?, parent_id = ?, is_orphaned = 0,
                 discovered_at = unixepoch()
         WHERE id = ?",
    )
    .bind(role_str(folder.role))
    .bind(folder.delimiter.as_deref())
    .bind(parent_id)
    .bind(id)
    .execute(&mut **tx)
    .await
    .map_err(|error| map_sqlx("reconcile_update", error))?;
    Ok(())
}

async fn insert_discovered(
    tx: &mut Transaction<'_, Sqlite>,
    account: AccountId,
    folder: &DiscoveredMailbox,
    parent_id: Option<i64>,
) -> Result<i64, MailError> {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO mailboxes
             (account_id, name, remote_name, role, parent_id, delimiter,
              is_subscribed, is_orphaned, discovered_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 0, unixepoch())
         RETURNING id",
    )
    .bind(account.value())
    .bind(folder.remote_name.as_str())
    .bind(folder.remote_name.as_str())
    .bind(role_str(folder.role))
    .bind(parent_id)
    .bind(folder.delimiter.as_deref())
    .bind(i64::from(folder.subscribed))
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| map_sqlx("reconcile_insert", error))
}

async fn message_count(
    tx: &mut Transaction<'_, Sqlite>,
    mailbox_id: i64,
) -> Result<i64, MailError> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages WHERE mailbox_id = ?")
        .bind(mailbox_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| map_sqlx("reconcile_message_count", error))
}

async fn flag_orphan(tx: &mut Transaction<'_, Sqlite>, id: i64) -> Result<(), MailError> {
    sqlx::query("UPDATE mailboxes SET is_orphaned = 1 WHERE id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|error| map_sqlx("reconcile_flag_orphan", error))?;
    Ok(())
}

async fn delete_row(tx: &mut Transaction<'_, Sqlite>, id: i64) -> Result<(), MailError> {
    sqlx::query("DELETE FROM mailboxes WHERE id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|error| map_sqlx("reconcile_delete", error))?;
    Ok(())
}
