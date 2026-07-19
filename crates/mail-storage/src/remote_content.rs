//! Per-sender remote-content allow-list repository (Track 1.6).
//!
//! Backs the `remote_content_senders` table (migration `0009`): the set of
//! sender addresses for which the reading pane loads remote images without
//! asking. Everything is keyed by a normalized (trimmed, lowercased) address
//! so a `From` header matches regardless of case, and lookups are primary-key
//! point queries. The default — an address absent from the table — is to keep
//! remote content blocked.

use axomail_core::MailError;
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;

/// Repository over the `remote_content_senders` allow-list table.
#[derive(Debug)]
pub struct RemoteContentRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> RemoteContentRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Normalizes a sender address for storage and lookup: surrounding
    /// whitespace removed and ASCII-lowercased, so `From` matches are
    /// case-insensitive.
    fn normalize(sender: &str) -> String {
        sender.trim().to_ascii_lowercase()
    }

    /// Returns whether remote content is allowed for `sender`.
    ///
    /// An empty or whitespace-only address is never allowed (and never stored).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn is_allowed(&self, sender: &str) -> Result<bool, MailError> {
        let key = Self::normalize(sender);
        if key.is_empty() {
            return Ok(false);
        }
        let found: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM remote_content_senders WHERE sender = ?")
                .bind(&key)
                .fetch_optional(self.pool)
                .await
                .map_err(|error| map_sqlx("is_remote_content_allowed", error))?;
        Ok(found.is_some())
    }

    /// Adds (`allowed = true`) or removes (`allowed = false`) `sender` from the
    /// allow list. Adding is idempotent; removing an absent sender is a no-op.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] when `sender` normalizes to the
    /// empty string, and [`MailError::Database`] on any query failure.
    pub async fn set_allowed(&self, sender: &str, allowed: bool) -> Result<(), MailError> {
        let key = Self::normalize(sender);
        if key.is_empty() {
            return Err(MailError::Configuration {
                message: "cannot set a remote-content policy for an empty sender address"
                    .to_owned(),
            });
        }
        if allowed {
            sqlx::query(
                "INSERT INTO remote_content_senders (sender) VALUES (?)
                 ON CONFLICT (sender) DO NOTHING",
            )
            .bind(&key)
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("allow_remote_content_sender", error))?;
        } else {
            sqlx::query("DELETE FROM remote_content_senders WHERE sender = ?")
                .bind(&key)
                .execute(self.pool)
                .await
                .map_err(|error| map_sqlx("disallow_remote_content_sender", error))?;
        }
        Ok(())
    }

    /// Lists every allowed sender, normalized and sorted. Primarily for tests
    /// and any future management view.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn allowed_senders(&self) -> Result<Vec<String>, MailError> {
        let rows: Vec<String> =
            sqlx::query_scalar("SELECT sender FROM remote_content_senders ORDER BY sender")
                .fetch_all(self.pool)
                .await
                .map_err(|error| map_sqlx("list_remote_content_senders", error))?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_trims_and_lowercases() {
        assert_eq!(
            RemoteContentRepo::normalize("  Alice@Example.ORG "),
            "alice@example.org"
        );
        assert_eq!(RemoteContentRepo::normalize(""), "");
        assert_eq!(RemoteContentRepo::normalize("   "), "");
    }
}
