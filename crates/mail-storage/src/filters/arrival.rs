//! Run-on-arrival bookkeeping for the filter engine.
//!
//! Each account has at most one `filter_arrival_state` row carrying the
//! per-account `enabled` toggle and the message-id high-water mark. Message ids
//! are monotonic, so the set of messages that arrived since the last pass is
//! exactly "inbox messages with id greater than `last_message_id`". Baselining
//! the mark to the current inbox frontier (rather than 0) the first time
//! filtering runs or is enabled is what keeps turning it on from retroactively
//! sweeping the existing inbox.

use axomail_core::{AccountId, MailError, MailboxId, MessageId};

use super::FilterRepo;
use crate::error_map::map_sqlx;

/// The per-account run-on-arrival state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrivalState {
    /// Whether arrival filtering is active for the account.
    pub enabled: bool,
    /// Highest inbox message id already considered for filtering.
    pub last_message_id: i64,
}

impl FilterRepo<'_> {
    /// Reads an account's arrival state, or `None` when it has never been
    /// baselined (no filtering has run and it has never been toggled).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn arrival_state(
        &self,
        account: AccountId,
    ) -> Result<Option<ArrivalState>, MailError> {
        let row = sqlx::query_as::<_, (bool, i64)>(
            "SELECT enabled, last_message_id FROM filter_arrival_state WHERE account_id = ?",
        )
        .bind(account.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("arrival_state", error))?;
        Ok(row.map(|(enabled, last_message_id)| ArrivalState {
            enabled,
            last_message_id,
        }))
    }

    /// Whether arrival filtering is enabled for an account. Defaults to `true`
    /// for an account that has never been baselined (filters, once created, run
    /// on new mail by default; the baseline keeps this from acting
    /// retroactively).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn arrival_enabled(&self, account: AccountId) -> Result<bool, MailError> {
        Ok(self
            .arrival_state(account)
            .await?
            .is_none_or(|state| state.enabled))
    }

    /// Creates the arrival-state row (baselined to `frontier`) if it does not
    /// already exist. A no-op when a row is present, so a genuine mark is never
    /// reset.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn baseline_arrival(
        &self,
        account: AccountId,
        frontier: i64,
    ) -> Result<(), MailError> {
        sqlx::query(
            "INSERT INTO filter_arrival_state (account_id, enabled, last_message_id)
             VALUES (?, 1, ?) ON CONFLICT (account_id) DO NOTHING",
        )
        .bind(account.value())
        .bind(frontier)
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("baseline_arrival", error))?;
        Ok(())
    }

    /// Sets the arrival `enabled` toggle, creating the row baselined to
    /// `frontier` if absent (so enabling never sweeps existing mail). An
    /// existing row keeps its high-water mark.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set_arrival_enabled(
        &self,
        account: AccountId,
        enabled: bool,
        frontier: i64,
    ) -> Result<(), MailError> {
        sqlx::query(
            "INSERT INTO filter_arrival_state (account_id, enabled, last_message_id)
             VALUES (?, ?, ?)
             ON CONFLICT (account_id) DO UPDATE SET enabled = excluded.enabled",
        )
        .bind(account.value())
        .bind(enabled)
        .bind(frontier)
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("set_arrival_enabled", error))?;
        Ok(())
    }

    /// Advances the high-water mark to `last_message_id` (never backwards).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn advance_arrival(
        &self,
        account: AccountId,
        last_message_id: i64,
    ) -> Result<(), MailError> {
        sqlx::query(
            "UPDATE filter_arrival_state SET last_message_id = ?
             WHERE account_id = ? AND last_message_id < ?",
        )
        .bind(last_message_id)
        .bind(account.value())
        .bind(last_message_id)
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("advance_arrival", error))?;
        Ok(())
    }

    /// Returns the ids of messages in `mailbox` with id greater than `after`,
    /// oldest (lowest id) first — the messages that arrived since the mark.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn messages_in_mailbox_after(
        &self,
        mailbox: MailboxId,
        after: i64,
    ) -> Result<Vec<MessageId>, MailError> {
        let ids = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM messages WHERE mailbox_id = ? AND id > ? ORDER BY id",
        )
        .bind(mailbox.value())
        .bind(after)
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("messages_in_mailbox_after", error))?;
        Ok(ids.into_iter().map(MessageId::new).collect())
    }

    /// The maximum message id currently in `mailbox` (0 when empty) — the
    /// baseline frontier for arrival filtering.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn max_message_id_in_mailbox(&self, mailbox: MailboxId) -> Result<i64, MailError> {
        let max = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(id), 0) FROM messages WHERE mailbox_id = ?",
        )
        .bind(mailbox.value())
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("max_message_id_in_mailbox", error))?;
        Ok(max)
    }
}
