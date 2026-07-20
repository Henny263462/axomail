//! Junk classifier repository: the per-account Bayesian token store, the
//! per-message score, the address-book whitelist guard, and the junk settings.
//!
//! The scoring math is [`axomail_core::junk`]; this repository is its
//! persistence. Training (`mark junk` / `not junk`) and scoring both work over a
//! deduplicated token set built with [`axomail_core::junk::message_tokens`], so
//! the two sides always see the message the same way. The connection-level SQL
//! lives in [`store`] so the ingest path can score inside its pass transaction.

mod store;

use std::collections::BTreeSet;

use axomail_core::junk::{Corpus, TokenStat};
use axomail_core::{AccountId, MailError, MessageId};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;

pub use store::JunkSettings;
pub(crate) use store::{
    is_pinned, is_whitelisted, load_settings, score_new_inbound, score_on, set_score_on,
    train_on_message,
};

/// Repository over `junk_tokens`, `junk_corpus`, and the `messages.junk_score`
/// column, plus the junk settings in `app_settings`.
#[derive(Debug)]
pub struct JunkRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> JunkRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Reads the current junk settings (defaults per field when unset).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn settings(&self) -> Result<JunkSettings, MailError> {
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|error| map_sqlx("junk_settings_acquire", error))?;
        load_settings(&mut conn).await
    }

    /// Persists the junk settings (threshold clamped to `0.0..=1.0`).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set_settings(&self, settings: &JunkSettings) -> Result<(), MailError> {
        let repo = crate::settings::SettingsRepo::new(self.pool);
        repo.set(store::KEY_ENABLED, bool_str(settings.enabled))
            .await?;
        repo.set(
            store::KEY_THRESHOLD,
            &settings.threshold.clamp(0.0, 1.0).to_string(),
        )
        .await?;
        repo.set(
            store::KEY_TRUST_HEADERS,
            bool_str(settings.trust_server_headers),
        )
        .await?;
        repo.set(
            store::KEY_WHITELIST_CONTACTS,
            bool_str(settings.whitelist_contacts),
        )
        .await?;
        repo.set(
            store::KEY_MARK_READ_ON_JUNK,
            bool_str(settings.mark_read_on_junk),
        )
        .await
    }

    /// Whether a message carries an explicit user junk pin (a mark-junk /
    /// not-junk training row). "Run junk controls" skips pinned messages so a
    /// re-score never overrides the user's explicit decision.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn is_pinned(
        &self,
        account: AccountId,
        message: MessageId,
    ) -> Result<bool, MailError> {
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|error| map_sqlx("junk_is_pinned_acquire", error))?;
        is_pinned(&mut conn, account, message.value()).await
    }

    /// Trains the account's classifier on one message's token set, as ham
    /// (`spam = false`) or spam (`spam = true`). Unconditional (non-idempotent):
    /// this is the low-level corpus-building primitive; the user-facing
    /// mark-junk / not-junk path uses [`train_message`](Self::train_message).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn train(
        &self,
        account: AccountId,
        tokens: &BTreeSet<String>,
        spam: bool,
    ) -> Result<(), MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("junk_train_begin", error))?;
        store::train_on(&mut tx, account, tokens, spam).await?;
        tx.commit()
            .await
            .map_err(|error| map_sqlx("junk_train_commit", error))
    }

    /// Idempotently trains the classifier on one message as ham/spam, keyed on a
    /// per-message ledger so repeat clicks and corrections never poison the
    /// model: re-training the same message in the same class is a no-op, and
    /// switching its class fully reverses the prior counts before applying the
    /// new ones. `tokens` must be the message's stable token set.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn train_message(
        &self,
        account: AccountId,
        message: MessageId,
        tokens: &BTreeSet<String>,
        spam: bool,
    ) -> Result<(), MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("junk_train_message_begin", error))?;
        train_on_message(&mut tx, account, message.value(), tokens, spam).await?;
        tx.commit()
            .await
            .map_err(|error| map_sqlx("junk_train_message_commit", error))
    }

    /// Scores a token set for `account`, returning a probability in
    /// `0.0..=1.0` ([`axomail_core::junk::NEUTRAL_SCORE`] with no training data).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn score(
        &self,
        account: AccountId,
        tokens: &BTreeSet<String>,
    ) -> Result<f64, MailError> {
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|error| map_sqlx("junk_score_acquire", error))?;
        score_on(&mut conn, account, tokens).await
    }

    /// Persists a message's computed junk score.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set_score(&self, message: MessageId, score: f64) -> Result<(), MailError> {
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|error| map_sqlx("junk_set_score_acquire", error))?;
        set_score_on(&mut conn, message.value(), score).await
    }

    /// Reads a message's stored junk score, or `None` if it has not been scored.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn score_of(&self, message: MessageId) -> Result<Option<f64>, MailError> {
        sqlx::query_scalar("SELECT junk_score FROM messages WHERE id = ?")
            .bind(message.value())
            .fetch_optional(self.pool)
            .await
            .map(Option::flatten)
            .map_err(|error| map_sqlx("junk_score_of", error))
    }

    /// The account-wide corpus totals (trained ham / spam message counts).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn corpus(&self, account: AccountId) -> Result<Corpus, MailError> {
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|error| map_sqlx("junk_corpus_acquire", error))?;
        store::load_corpus(&mut conn, account).await
    }

    /// One token's stored ham/spam counts for `account` (zeroed when unseen).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn token_stat(
        &self,
        account: AccountId,
        token: &str,
    ) -> Result<TokenStat, MailError> {
        let row: Option<(i64, i64)> = sqlx::query_as(
            "SELECT ham_count, spam_count FROM junk_tokens WHERE account_id = ? AND token = ?",
        )
        .bind(account.value())
        .bind(token)
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("junk_token_stat", error))?;
        Ok(row.map_or(TokenStat::default(), |(ham, spam)| TokenStat {
            ham: u32::try_from(ham).unwrap_or(u32::MAX),
            spam: u32::try_from(spam).unwrap_or(u32::MAX),
        }))
    }

    /// Whether `email` appears in the global address book (contact whitelist).
    ///
    /// The address book is global rather than per-account, so a sender saved as
    /// a contact under any account is whitelisted; matching is case-insensitive.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn is_whitelisted_sender(&self, email: &str) -> Result<bool, MailError> {
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|error| map_sqlx("junk_whitelist_acquire", error))?;
        is_whitelisted(&mut conn, email).await
    }
}

/// Canonical string form of a boolean setting.
fn bool_str(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}
