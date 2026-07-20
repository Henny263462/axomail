//! Junk classifier training: the idempotent, reversible per-message training,
//! the explicit-pin check, and the low-level corpus/token delta primitives.
//!
//! The user-facing mark-junk / not-junk path uses [`train_on_message`], keyed on
//! the `junk_training` ledger so repeat clicks and corrections never poison the
//! corpus; [`train_on`] is the unconditional primitive used to build a corpus
//! from many messages (and by tests). [`is_pinned`] reports whether a message
//! carries an explicit user pin (a ledger row), which "run junk controls" must
//! not override.

use std::collections::BTreeSet;

use axomail_core::{AccountId, MailError};
use sqlx::SqliteConnection;

use crate::error_map::map_sqlx;

/// Trains `account` on one message's token set (unconditionally): bumps the
/// corpus total for the class and, per distinct token, its ham or spam count.
///
/// This is the low-level, non-idempotent primitive (used to build a corpus from
/// many messages). The user-facing mark-junk / not-junk path goes through
/// [`train_on_message`], which is idempotent and reversible.
pub(crate) async fn train_on(
    conn: &mut SqliteConnection,
    account: AccountId,
    tokens: &BTreeSet<String>,
    spam: bool,
) -> Result<(), MailError> {
    let (ham_delta, spam_delta): (i64, i64) = if spam { (0, 1) } else { (1, 0) };
    apply_corpus_delta(conn, account, ham_delta, spam_delta).await?;
    apply_token_deltas(conn, account, tokens, ham_delta, spam_delta).await
}

/// Idempotently trains `account` on one message as ham or spam, keyed on the
/// per-message training ledger so repeat clicks and corrections cannot poison
/// the corpus:
///
/// - re-training the same message in the same class is a **no-op**;
/// - switching a message's class **reverses** the prior class's counts (never
///   below zero) and applies the new class's — a true correction.
///
/// `tokens` must be the message's stable token set (the same one used to score
/// it); a reversal re-applies it to the class being undone.
pub(crate) async fn train_on_message(
    conn: &mut SqliteConnection,
    account: AccountId,
    message_id: i64,
    tokens: &BTreeSet<String>,
    spam: bool,
) -> Result<(), MailError> {
    let prior: Option<i64> = sqlx::query_scalar(
        "SELECT is_spam FROM junk_training WHERE account_id = ? AND message_id = ?",
    )
    .bind(account.value())
    .bind(message_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|error| map_sqlx("junk_training_lookup", error))?;
    let prior_spam = prior.map(|value| value != 0);
    if prior_spam == Some(spam) {
        return Ok(()); // Already trained in this class: idempotent no-op.
    }

    // Net deltas: undo the prior class (if any), apply the new one. A reversal
    // implies a prior training, which created the corpus/token rows, so the
    // negative side always lands on an existing (clamped) row.
    let (old_ham, old_spam) = match prior_spam {
        Some(true) => (0i64, 1i64),
        Some(false) => (1, 0),
        None => (0, 0),
    };
    let (new_ham, new_spam) = if spam { (0, 1) } else { (1, 0) };
    let ham_delta = new_ham - old_ham;
    let spam_delta = new_spam - old_spam;

    apply_corpus_delta(conn, account, ham_delta, spam_delta).await?;
    apply_token_deltas(conn, account, tokens, ham_delta, spam_delta).await?;

    sqlx::query(
        "INSERT INTO junk_training (account_id, message_id, is_spam) VALUES (?, ?, ?)
         ON CONFLICT(account_id, message_id) DO UPDATE SET is_spam = excluded.is_spam",
    )
    .bind(account.value())
    .bind(message_id)
    .bind(i64::from(spam))
    .execute(&mut *conn)
    .await
    .map_err(|error| map_sqlx("junk_training_record", error))?;
    Ok(())
}

/// Whether a message currently has an explicit user junk pin — a
/// [`train_on_message`] ledger row (mark-junk / not-junk). "Run junk controls"
/// must skip these so a re-score never overrides a user's explicit decision.
pub(crate) async fn is_pinned(
    conn: &mut SqliteConnection,
    account: AccountId,
    message_id: i64,
) -> Result<bool, MailError> {
    let found: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM junk_training WHERE account_id = ? AND message_id = ? LIMIT 1",
    )
    .bind(account.value())
    .bind(message_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|error| map_sqlx("junk_is_pinned", error))?;
    Ok(found.is_some())
}

/// Applies signed deltas to the account's corpus totals, clamping each at zero
/// so a reversal can never drive a count negative.
async fn apply_corpus_delta(
    conn: &mut SqliteConnection,
    account: AccountId,
    ham_delta: i64,
    spam_delta: i64,
) -> Result<(), MailError> {
    sqlx::query(
        "INSERT INTO junk_corpus (account_id, ham_messages, spam_messages) VALUES (?, ?, ?)
         ON CONFLICT(account_id) DO UPDATE SET
             ham_messages = MAX(ham_messages + excluded.ham_messages, 0),
             spam_messages = MAX(spam_messages + excluded.spam_messages, 0)",
    )
    .bind(account.value())
    .bind(ham_delta)
    .bind(spam_delta)
    .execute(&mut *conn)
    .await
    .map_err(|error| map_sqlx("junk_train_corpus", error))?;
    Ok(())
}

/// Applies signed deltas to each token's counts, clamping each at zero.
async fn apply_token_deltas(
    conn: &mut SqliteConnection,
    account: AccountId,
    tokens: &BTreeSet<String>,
    ham_delta: i64,
    spam_delta: i64,
) -> Result<(), MailError> {
    for token in tokens {
        sqlx::query(
            "INSERT INTO junk_tokens (account_id, token, ham_count, spam_count) VALUES (?, ?, ?, ?)
             ON CONFLICT(account_id, token) DO UPDATE SET
                 ham_count = MAX(ham_count + excluded.ham_count, 0),
                 spam_count = MAX(spam_count + excluded.spam_count, 0)",
        )
        .bind(account.value())
        .bind(token)
        .bind(ham_delta)
        .bind(spam_delta)
        .execute(&mut *conn)
        .await
        .map_err(|error| map_sqlx("junk_train_token", error))?;
    }
    Ok(())
}
