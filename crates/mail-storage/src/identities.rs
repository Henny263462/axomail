//! Identity repository: the per-account From identities.
//!
//! Every account owns one or more identities and exactly one of them is the
//! default. That invariant is enforced from two sides: a partial unique index
//! (`idx_identities_one_default`) guarantees **at most** one default per
//! account, and this repository guarantees **at least** one by seeding a
//! default on the first insert and promoting a survivor when the default is
//! deleted. All default transitions run inside a transaction so a reader never
//! observes zero or two defaults.

use axomail_core::{AccountId, MailError, MailboxId};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;
use crate::identity_prefs::{ArchiveGranularity, ForwardMode, ReplyPosition, SignaturePlacement};

/// Identifier of a persisted [`Identity`] row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdentityId(i64);

impl IdentityId {
    /// Wraps a raw database key.
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    /// Returns the raw database key.
    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
    }
}

impl std::fmt::Display for IdentityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Mutable content shared by inserts and updates of an identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityFields {
    /// From address (stored as a plain string; validated at the command edge).
    pub email: String,
    /// Display name shown in the From header; may be empty.
    pub display_name: String,
    /// Optional Reply-To address.
    pub reply_to: Option<String>,
    /// Optional plain-text signature (without the leading `-- ` delimiter).
    pub signature_text: Option<String>,
    /// Where the signature sits relative to a quote.
    pub signature_placement: SignaturePlacement,
    /// Where a reply body is typed relative to a quote.
    pub reply_position: ReplyPosition,
    /// How forwards carry the original.
    pub forward_mode: ForwardMode,
    /// Archive sub-folder granularity.
    pub archive_granularity: ArchiveGranularity,
    /// Mailbox that sent copies are filed into (Wave 3), if chosen.
    pub sent_dest: Option<MailboxId>,
    /// Mailbox that drafts are filed into (Wave 3), if chosen.
    pub drafts_dest: Option<MailboxId>,
    /// Mailbox that templates are filed into (Wave 3), if chosen.
    pub templates_dest: Option<MailboxId>,
    /// Mailbox that archives are filed into (Wave 3), if chosen.
    pub archive_dest: Option<MailboxId>,
}

/// A new identity to insert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewIdentity {
    /// Owning account.
    pub account: AccountId,
    /// Editable content.
    pub fields: IdentityFields,
    /// Request that this identity become the account default. Ignored (forced
    /// true) when it is the account's first identity.
    pub make_default: bool,
    /// Ordering position within the account's identity list.
    pub sort_position: i64,
}

/// A persisted identity row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// Row identifier.
    pub id: IdentityId,
    /// Owning account.
    pub account: AccountId,
    /// Editable content.
    pub fields: IdentityFields,
    /// Whether this is the account's default identity.
    pub is_default: bool,
    /// Ordering position within the account's identity list.
    pub sort_position: i64,
}

/// Columns selected when reading an identity, in [`IdentityRow`] order.
const IDENTITY_COLUMNS: &str = "id, account_id, email, display_name, reply_to, \
    signature_text, signature_placement, reply_position, forward_mode, \
    archive_granularity, sent_dest, drafts_dest, templates_dest, archive_dest, \
    is_default, sort_position";

/// Repository over the `identities` table.
#[derive(Debug)]
pub struct IdentityRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> IdentityRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Inserts an identity, returning it with its assigned id.
    ///
    /// Becomes the account default when requested or when it is the account's
    /// first identity; the default flip and the insert share one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn insert(&self, new: &NewIdentity) -> Result<Identity, MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| map_sqlx("insert_identity", e))?;

        let existing: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM identities WHERE account_id = ?")
                .bind(new.account.value())
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| map_sqlx("insert_identity", e))?;
        let make_default = new.make_default || existing == 0;
        if make_default {
            clear_default(&mut tx, new.account.value()).await?;
        }

        let f = &new.fields;
        let row = sqlx::query_as::<_, IdentityRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO identities (
                account_id, email, display_name, reply_to, signature_text,
                signature_placement, reply_position, forward_mode, archive_granularity,
                sent_dest, drafts_dest, templates_dest, archive_dest,
                is_default, sort_position
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING {IDENTITY_COLUMNS}"
        )))
        .bind(new.account.value())
        .bind(f.email.as_str())
        .bind(f.display_name.as_str())
        .bind(f.reply_to.as_deref())
        .bind(f.signature_text.as_deref())
        .bind(f.signature_placement.as_str())
        .bind(f.reply_position.as_str())
        .bind(f.forward_mode.as_str())
        .bind(f.archive_granularity.as_str())
        .bind(f.sent_dest.map(MailboxId::value))
        .bind(f.drafts_dest.map(MailboxId::value))
        .bind(f.templates_dest.map(MailboxId::value))
        .bind(f.archive_dest.map(MailboxId::value))
        .bind(i64::from(make_default))
        .bind(new.sort_position)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx("insert_identity", e))?;

        tx.commit()
            .await
            .map_err(|e| map_sqlx("insert_identity", e))?;
        row.into_identity()
    }

    /// Fetches an identity by id, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn get(&self, id: IdentityId) -> Result<Option<Identity>, MailError> {
        let row = sqlx::query_as::<_, IdentityRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {IDENTITY_COLUMNS} FROM identities WHERE id = ?"
        )))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|e| map_sqlx("get_identity", e))?;
        row.map(IdentityRow::into_identity).transpose()
    }

    /// Lists one account's identities, default first, then by sort position.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn list(&self, account: AccountId) -> Result<Vec<Identity>, MailError> {
        let rows = sqlx::query_as::<_, IdentityRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {IDENTITY_COLUMNS} FROM identities WHERE account_id = ? \
             ORDER BY is_default DESC, sort_position ASC, id ASC"
        )))
        .bind(account.value())
        .fetch_all(self.pool)
        .await
        .map_err(|e| map_sqlx("list_identities", e))?;
        rows.into_iter().map(IdentityRow::into_identity).collect()
    }

    /// Lists every identity across all accounts, grouped by account then
    /// default-first, for the composer's From picker.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn list_all(&self) -> Result<Vec<Identity>, MailError> {
        let rows = sqlx::query_as::<_, IdentityRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {IDENTITY_COLUMNS} FROM identities \
             ORDER BY account_id ASC, is_default DESC, sort_position ASC, id ASC"
        )))
        .fetch_all(self.pool)
        .await
        .map_err(|e| map_sqlx("list_all_identities", e))?;
        rows.into_iter().map(IdentityRow::into_identity).collect()
    }

    /// Overwrites an identity's editable content, leaving its default flag and
    /// account untouched. Returns `true` if the identity existed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn update(&self, id: IdentityId, fields: &IdentityFields) -> Result<bool, MailError> {
        let result = sqlx::query(
            "UPDATE identities SET
                 email = ?, display_name = ?, reply_to = ?, signature_text = ?,
                 signature_placement = ?, reply_position = ?, forward_mode = ?,
                 archive_granularity = ?, sent_dest = ?, drafts_dest = ?,
                 templates_dest = ?, archive_dest = ?
             WHERE id = ?",
        )
        .bind(fields.email.as_str())
        .bind(fields.display_name.as_str())
        .bind(fields.reply_to.as_deref())
        .bind(fields.signature_text.as_deref())
        .bind(fields.signature_placement.as_str())
        .bind(fields.reply_position.as_str())
        .bind(fields.forward_mode.as_str())
        .bind(fields.archive_granularity.as_str())
        .bind(fields.sent_dest.map(MailboxId::value))
        .bind(fields.drafts_dest.map(MailboxId::value))
        .bind(fields.templates_dest.map(MailboxId::value))
        .bind(fields.archive_dest.map(MailboxId::value))
        .bind(id.value())
        .execute(self.pool)
        .await
        .map_err(|e| map_sqlx("update_identity", e))?;
        Ok(result.rows_affected() > 0)
    }

    /// Makes `id` its account's default, clearing the previous default in the
    /// same transaction. Returns `true` if the identity existed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set_default(&self, id: IdentityId) -> Result<bool, MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| map_sqlx("set_default_identity", e))?;
        let account: Option<i64> =
            sqlx::query_scalar("SELECT account_id FROM identities WHERE id = ?")
                .bind(id.value())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| map_sqlx("set_default_identity", e))?;
        let Some(account) = account else {
            return Ok(false);
        };
        clear_default(&mut tx, account).await?;
        sqlx::query("UPDATE identities SET is_default = 1 WHERE id = ?")
            .bind(id.value())
            .execute(&mut *tx)
            .await
            .map_err(|e| map_sqlx("set_default_identity", e))?;
        tx.commit()
            .await
            .map_err(|e| map_sqlx("set_default_identity", e))?;
        Ok(true)
    }

    /// Deletes an identity. When the deleted row was the account default and
    /// other identities remain, the first survivor is promoted to default in
    /// the same transaction. Returns `true` if a row was removed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete(&self, id: IdentityId) -> Result<bool, MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| map_sqlx("delete_identity", e))?;
        let target: Option<(i64, i64)> =
            sqlx::query_as("SELECT account_id, is_default FROM identities WHERE id = ?")
                .bind(id.value())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| map_sqlx("delete_identity", e))?;
        let Some((account, was_default)) = target else {
            return Ok(false);
        };
        sqlx::query("DELETE FROM identities WHERE id = ?")
            .bind(id.value())
            .execute(&mut *tx)
            .await
            .map_err(|e| map_sqlx("delete_identity", e))?;
        if was_default == 1 {
            // A NULL subquery (no survivor) matches no row, leaving the account
            // with zero identities rather than erroring.
            sqlx::query(
                "UPDATE identities SET is_default = 1 WHERE id = (
                     SELECT id FROM identities WHERE account_id = ?
                     ORDER BY sort_position ASC, id ASC LIMIT 1
                 )",
            )
            .bind(account)
            .execute(&mut *tx)
            .await
            .map_err(|e| map_sqlx("delete_identity", e))?;
        }
        tx.commit()
            .await
            .map_err(|e| map_sqlx("delete_identity", e))?;
        Ok(true)
    }
}

/// Clears any default flag for one account within a transaction.
async fn clear_default(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account: i64,
) -> Result<(), MailError> {
    sqlx::query("UPDATE identities SET is_default = 0 WHERE account_id = ? AND is_default = 1")
        .bind(account)
        .execute(&mut **tx)
        .await
        .map_err(|e| map_sqlx("clear_default_identity", e))?;
    Ok(())
}

/// Raw `identities` row, mapped by column name.
#[derive(sqlx::FromRow)]
struct IdentityRow {
    id: i64,
    account_id: i64,
    email: String,
    display_name: String,
    reply_to: Option<String>,
    signature_text: Option<String>,
    signature_placement: String,
    reply_position: String,
    forward_mode: String,
    archive_granularity: String,
    sent_dest: Option<i64>,
    drafts_dest: Option<i64>,
    templates_dest: Option<i64>,
    archive_dest: Option<i64>,
    is_default: i64,
    sort_position: i64,
}

impl IdentityRow {
    fn into_identity(self) -> Result<Identity, MailError> {
        Ok(Identity {
            id: IdentityId::new(self.id),
            account: AccountId::new(self.account_id),
            fields: IdentityFields {
                email: self.email,
                display_name: self.display_name,
                reply_to: self.reply_to,
                signature_text: self.signature_text,
                signature_placement: SignaturePlacement::parse(&self.signature_placement)?,
                reply_position: ReplyPosition::parse(&self.reply_position)?,
                forward_mode: ForwardMode::parse(&self.forward_mode)?,
                archive_granularity: ArchiveGranularity::parse(&self.archive_granularity)?,
                sent_dest: self.sent_dest.map(MailboxId::new),
                drafts_dest: self.drafts_dest.map(MailboxId::new),
                templates_dest: self.templates_dest.map(MailboxId::new),
                archive_dest: self.archive_dest.map(MailboxId::new),
            },
            is_default: self.is_default != 0,
            sort_position: self.sort_position,
        })
    }
}
