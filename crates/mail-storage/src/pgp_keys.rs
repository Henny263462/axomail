//! PGP key repository: non-secret metadata for our own keys and for recipient
//! (contact) public keys.
//!
//! Secret key material is never stored here — an own key's armored secret half
//! lives in the OS keychain (ADR-0001), keyed by fingerprint; this table holds
//! only the public half and metadata. Recipient keys are all-public by nature.
//! The command layer owns the keychain writes; the repository owns the database
//! rows.

mod types;

use axomail_core::MailError;
use sqlx::SqlitePool;

pub use types::{
    NewPgpOwnKey, NewPgpRecipientKey, PgpOwnKey, PgpOwnKeyId, PgpRecipientKey, PgpRecipientKeyId,
};

use crate::error_map::map_sqlx;

/// Columns of `pgp_own_keys`, in [`OwnKeyRow`] order.
const OWN_COLUMNS: &str = "id, fingerprint, key_id, user_id, algorithm, public_armored, \
     has_passphrase, is_default, created_at_unix, added_at_unix";

/// Columns of `pgp_recipient_keys`, in [`RecipientKeyRow`] order.
const RECIPIENT_COLUMNS: &str = "id, email, fingerprint, key_id, user_id, algorithm, \
     public_armored, created_at_unix, added_at_unix";

/// Repository over `pgp_own_keys` and `pgp_recipient_keys`.
#[derive(Debug)]
pub struct PgpKeyRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> PgpKeyRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    // -- Own keys ---------------------------------------------------------

    /// Inserts an own key. If it is the first key, it becomes the default.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure, including the
    /// unique-fingerprint constraint when the key already exists.
    pub async fn insert_own(&self, new: &NewPgpOwnKey) -> Result<PgpOwnKey, MailError> {
        let is_first: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pgp_own_keys")
            .fetch_one(self.pool)
            .await
            .map_err(|error| map_sqlx("count_own_pgp_keys", error))?;
        let row = sqlx::query_as::<_, OwnKeyRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO pgp_own_keys
                 (fingerprint, key_id, user_id, algorithm, public_armored,
                  has_passphrase, is_default, created_at_unix, added_at_unix)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             RETURNING {OWN_COLUMNS}"
        )))
        .bind(&new.fingerprint)
        .bind(&new.key_id)
        .bind(&new.user_id)
        .bind(&new.algorithm)
        .bind(&new.public_armored)
        .bind(i64::from(new.has_passphrase))
        .bind(i64::from(is_first == 0))
        .bind(new.created_at_unix)
        .bind(new.added_at_unix)
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("insert_own_pgp_key", error))?;
        Ok(row.into_key())
    }

    /// Lists all own keys, default first then newest.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn list_own(&self) -> Result<Vec<PgpOwnKey>, MailError> {
        let rows = sqlx::query_as::<_, OwnKeyRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {OWN_COLUMNS} FROM pgp_own_keys ORDER BY is_default DESC, id DESC"
        )))
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_own_pgp_keys", error))?;
        Ok(rows.into_iter().map(OwnKeyRow::into_key).collect())
    }

    /// Fetches one own key by id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn get_own(&self, id: PgpOwnKeyId) -> Result<Option<PgpOwnKey>, MailError> {
        let row = sqlx::query_as::<_, OwnKeyRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {OWN_COLUMNS} FROM pgp_own_keys WHERE id = ?"
        )))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_own_pgp_key", error))?;
        Ok(row.map(OwnKeyRow::into_key))
    }

    /// The default signing key, if any own key exists.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn default_own(&self) -> Result<Option<PgpOwnKey>, MailError> {
        let row = sqlx::query_as::<_, OwnKeyRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {OWN_COLUMNS} FROM pgp_own_keys
             ORDER BY is_default DESC, id DESC LIMIT 1"
        )))
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("default_own_pgp_key", error))?;
        Ok(row.map(OwnKeyRow::into_key))
    }

    /// The own key whose user id contains `email`, else the default key. Used to
    /// pick the signing key that matches the composer's From address.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn own_for_email(&self, email: &str) -> Result<Option<PgpOwnKey>, MailError> {
        let pattern = format!("%{}%", email.to_lowercase());
        let row = sqlx::query_as::<_, OwnKeyRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {OWN_COLUMNS} FROM pgp_own_keys
             WHERE LOWER(user_id) LIKE ? ORDER BY is_default DESC, id DESC LIMIT 1"
        )))
        .bind(pattern)
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("own_pgp_key_for_email", error))?;
        match row {
            Some(row) => Ok(Some(row.into_key())),
            None => self.default_own().await,
        }
    }

    /// The own key whose user id contains `email`, with **no** fallback to the
    /// default — used to decide whether a specific address is one of ours (e.g.
    /// encrypt-to-self), where the default-key fallback of [`own_for_email`] would
    /// wrongly match every address.
    ///
    /// [`own_for_email`]: Self::own_for_email
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn own_key_for_address(&self, email: &str) -> Result<Option<PgpOwnKey>, MailError> {
        let pattern = format!("%{}%", email.to_lowercase());
        let row = sqlx::query_as::<_, OwnKeyRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {OWN_COLUMNS} FROM pgp_own_keys
             WHERE LOWER(user_id) LIKE ? ORDER BY is_default DESC, id DESC LIMIT 1"
        )))
        .bind(pattern)
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("own_key_for_address", error))?;
        Ok(row.map(OwnKeyRow::into_key))
    }

    /// Makes `id` the default signing key, clearing the flag on all others.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set_default_own(&self, id: PgpOwnKeyId) -> Result<(), MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("set_default_own_begin", error))?;
        sqlx::query("UPDATE pgp_own_keys SET is_default = 0")
            .execute(&mut *tx)
            .await
            .map_err(|error| map_sqlx("clear_default_own", error))?;
        sqlx::query("UPDATE pgp_own_keys SET is_default = 1 WHERE id = ?")
            .bind(id.value())
            .execute(&mut *tx)
            .await
            .map_err(|error| map_sqlx("set_default_own", error))?;
        tx.commit()
            .await
            .map_err(|error| map_sqlx("set_default_own_commit", error))
    }

    /// Deletes an own key row, returning it so the caller can purge the matching
    /// keychain secret. Returns `None` if no such key exists. If the deleted key
    /// was the default, the newest remaining key is promoted.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete_own(&self, id: PgpOwnKeyId) -> Result<Option<PgpOwnKey>, MailError> {
        let row = sqlx::query_as::<_, OwnKeyRow>(sqlx::AssertSqlSafe(format!(
            "DELETE FROM pgp_own_keys WHERE id = ? RETURNING {OWN_COLUMNS}"
        )))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("delete_own_pgp_key", error))?;
        let Some(row) = row else { return Ok(None) };
        let deleted = row.into_key();
        if deleted.is_default {
            sqlx::query(
                "UPDATE pgp_own_keys SET is_default = 1
                 WHERE id = (SELECT id FROM pgp_own_keys ORDER BY id DESC LIMIT 1)",
            )
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("promote_default_own_pgp_key", error))?;
        }
        Ok(Some(deleted))
    }

    // -- Recipient keys ---------------------------------------------------

    /// Inserts or replaces a recipient key for `(email, fingerprint)`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn upsert_recipient(
        &self,
        new: &NewPgpRecipientKey,
    ) -> Result<PgpRecipientKey, MailError> {
        let row = sqlx::query_as::<_, RecipientKeyRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO pgp_recipient_keys
                 (email, fingerprint, key_id, user_id, algorithm, public_armored,
                  created_at_unix, added_at_unix)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (email, fingerprint) DO UPDATE SET
                 key_id = excluded.key_id,
                 user_id = excluded.user_id,
                 algorithm = excluded.algorithm,
                 public_armored = excluded.public_armored,
                 created_at_unix = excluded.created_at_unix,
                 added_at_unix = excluded.added_at_unix
             RETURNING {RECIPIENT_COLUMNS}"
        )))
        .bind(new.email.to_lowercase())
        .bind(&new.fingerprint)
        .bind(&new.key_id)
        .bind(&new.user_id)
        .bind(&new.algorithm)
        .bind(&new.public_armored)
        .bind(new.created_at_unix)
        .bind(new.added_at_unix)
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("upsert_recipient_pgp_key", error))?;
        Ok(row.into_key())
    }

    /// Lists all recipient keys, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn list_recipients(&self) -> Result<Vec<PgpRecipientKey>, MailError> {
        let rows = sqlx::query_as::<_, RecipientKeyRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {RECIPIENT_COLUMNS} FROM pgp_recipient_keys ORDER BY email, id DESC"
        )))
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_recipient_pgp_keys", error))?;
        Ok(rows.into_iter().map(RecipientKeyRow::into_key).collect())
    }

    /// Deletes a recipient key. Returns `true` if a row was removed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete_recipient(&self, id: PgpRecipientKeyId) -> Result<bool, MailError> {
        let result = sqlx::query("DELETE FROM pgp_recipient_keys WHERE id = ?")
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("delete_recipient_pgp_key", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Returns the newest armored public key held for `email`, or `None`.
    ///
    /// An own key whose user id matches is also acceptable — encrypting to
    /// oneself so the Sent copy is readable — but that is resolved by the caller;
    /// this looks only at recipient keys.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn recipient_public_for(&self, email: &str) -> Result<Option<String>, MailError> {
        let armored: Option<String> = sqlx::query_scalar(
            "SELECT public_armored FROM pgp_recipient_keys
             WHERE email = ? ORDER BY id DESC LIMIT 1",
        )
        .bind(email.to_lowercase())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("recipient_public_for", error))?;
        Ok(armored)
    }

    /// Whether at least one recipient key is held for `email`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn has_recipient_key(&self, email: &str) -> Result<bool, MailError> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pgp_recipient_keys WHERE email = ?")
                .bind(email.to_lowercase())
                .fetch_one(self.pool)
                .await
                .map_err(|error| map_sqlx("has_recipient_key", error))?;
        Ok(count > 0)
    }
}

/// Raw `pgp_own_keys` row.
#[derive(sqlx::FromRow)]
struct OwnKeyRow {
    id: i64,
    fingerprint: String,
    key_id: String,
    user_id: String,
    algorithm: String,
    public_armored: String,
    has_passphrase: i64,
    is_default: i64,
    created_at_unix: i64,
    added_at_unix: i64,
}

impl OwnKeyRow {
    fn into_key(self) -> PgpOwnKey {
        PgpOwnKey {
            id: PgpOwnKeyId::new(self.id),
            fingerprint: self.fingerprint,
            key_id: self.key_id,
            user_id: self.user_id,
            algorithm: self.algorithm,
            public_armored: self.public_armored,
            has_passphrase: self.has_passphrase != 0,
            is_default: self.is_default != 0,
            created_at_unix: self.created_at_unix,
            added_at_unix: self.added_at_unix,
        }
    }
}

/// Raw `pgp_recipient_keys` row.
#[derive(sqlx::FromRow)]
struct RecipientKeyRow {
    id: i64,
    email: String,
    fingerprint: String,
    key_id: String,
    user_id: String,
    algorithm: String,
    public_armored: String,
    created_at_unix: i64,
    added_at_unix: i64,
}

impl RecipientKeyRow {
    fn into_key(self) -> PgpRecipientKey {
        PgpRecipientKey {
            id: PgpRecipientKeyId::new(self.id),
            email: self.email,
            fingerprint: self.fingerprint,
            key_id: self.key_id,
            user_id: self.user_id,
            algorithm: self.algorithm,
            public_armored: self.public_armored,
            created_at_unix: self.created_at_unix,
            added_at_unix: self.added_at_unix,
        }
    }
}
