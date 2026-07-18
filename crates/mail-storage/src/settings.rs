//! Settings repository: a simple string key/value store backed by the
//! `app_settings` table.

use axomail_core::MailError;
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;

/// Repository over the `app_settings` table.
#[derive(Debug)]
pub struct SettingsRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> SettingsRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Reads the value for `key`, or `None` if it is not set.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn get(&self, key: &str) -> Result<Option<String>, MailError> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
                .bind(key)
                .fetch_optional(self.pool)
                .await
                .map_err(|error| map_sqlx("get_setting", error))?;
        Ok(value)
    }

    /// Sets `key` to `value`, inserting or overwriting any prior value.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set(&self, key: &str, value: &str) -> Result<(), MailError> {
        sqlx::query(
            "INSERT INTO app_settings (key, value) VALUES (?, ?)
             ON CONFLICT (key) DO UPDATE SET
                 value = excluded.value,
                 updated_at_unix = unixepoch()",
        )
        .bind(key)
        .bind(value)
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("set_setting", error))?;
        Ok(())
    }
}
