//! Connection-level SQL for the junk classifier's token store and settings.
//!
//! These free functions run on any `&mut SqliteConnection`, so the ingest
//! scoring path calls them inside its pass transaction (score + persist land
//! atomically with the envelope insert) while [`JunkRepo`](super::JunkRepo)
//! calls them on a pooled connection for the command surface. The classifier
//! math itself lives in [`axomail_core::junk`]; this layer only supplies the
//! per-account corpus counts it needs.

use std::collections::BTreeSet;

use axomail_core::junk::{self, Corpus, TokenStat, message_tokens};
use axomail_core::{AccountId, MailError, MessageId, RemoteEnvelope};
use sqlx::SqliteConnection;

use crate::error_map::map_sqlx;

/// Setting key: whether inbound scoring and auto-routing run at all.
pub(crate) const KEY_ENABLED: &str = "junk.enabled";
/// Setting key: the score at or above which a message is treated as junk.
pub(crate) const KEY_THRESHOLD: &str = "junk.threshold";
/// Setting key: honor a server's `X-Spam-*` verdict when the raw is available.
pub(crate) const KEY_TRUST_HEADERS: &str = "junk.trust_server_headers";
/// Setting key: never junk a sender in the address book (track 4.1).
pub(crate) const KEY_WHITELIST_CONTACTS: &str = "junk.whitelist_contacts";

/// Default: opt-in, so a fresh install never auto-files before the user asks.
pub(crate) const DEFAULT_ENABLED: bool = false;
/// Default junk threshold: a strong 0.9 lean toward spam.
pub(crate) const DEFAULT_THRESHOLD: f64 = 0.9;
/// Server spam headers are trusted by default when present.
pub(crate) const DEFAULT_TRUST_HEADERS: bool = true;
/// The address-book whitelist is honored by default.
pub(crate) const DEFAULT_WHITELIST_CONTACTS: bool = true;

/// User-tunable junk controls, read from the shared `app_settings` table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JunkSettings {
    /// Whether inbound scoring + auto-routing are active.
    pub enabled: bool,
    /// Score threshold (clamped to `0.0..=1.0`) for treating a message as junk.
    pub threshold: f64,
    /// Whether a server's `X-Spam-*` verdict is honored.
    pub trust_server_headers: bool,
    /// Whether address-book senders are exempt from junking.
    pub whitelist_contacts: bool,
}

impl Default for JunkSettings {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_ENABLED,
            threshold: DEFAULT_THRESHOLD,
            trust_server_headers: DEFAULT_TRUST_HEADERS,
            whitelist_contacts: DEFAULT_WHITELIST_CONTACTS,
        }
    }
}

/// Reads a single `app_settings` value on this connection.
async fn setting(conn: &mut SqliteConnection, key: &str) -> Result<Option<String>, MailError> {
    sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|error| map_sqlx("junk_setting", error))
}

/// Loads the junk settings, falling back to the documented defaults per field.
pub(crate) async fn load_settings(conn: &mut SqliteConnection) -> Result<JunkSettings, MailError> {
    let enabled = setting(conn, KEY_ENABLED)
        .await?
        .map_or(DEFAULT_ENABLED, |value| value == "true");
    let threshold = setting(conn, KEY_THRESHOLD)
        .await?
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(DEFAULT_THRESHOLD)
        .clamp(0.0, 1.0);
    let trust_server_headers = setting(conn, KEY_TRUST_HEADERS)
        .await?
        .map_or(DEFAULT_TRUST_HEADERS, |value| value == "true");
    let whitelist_contacts = setting(conn, KEY_WHITELIST_CONTACTS)
        .await?
        .map_or(DEFAULT_WHITELIST_CONTACTS, |value| value == "true");
    Ok(JunkSettings {
        enabled,
        threshold,
        trust_server_headers,
        whitelist_contacts,
    })
}

/// Reads the per-account corpus totals (zeroed when the account has no row).
pub(crate) async fn load_corpus(
    conn: &mut SqliteConnection,
    account: AccountId,
) -> Result<Corpus, MailError> {
    let row: Option<(i64, i64)> =
        sqlx::query_as("SELECT ham_messages, spam_messages FROM junk_corpus WHERE account_id = ?")
            .bind(account.value())
            .fetch_optional(&mut *conn)
            .await
            .map_err(|error| map_sqlx("junk_load_corpus", error))?;
    Ok(row.map_or(Corpus::default(), |(ham, spam)| Corpus {
        ham_messages: clamp_u32(ham),
        spam_messages: clamp_u32(spam),
    }))
}

/// Loads the stats for the tokens present in the store (missing tokens carry no
/// evidence, so they are simply absent — the classifier ignores them).
async fn load_present_stats(
    conn: &mut SqliteConnection,
    account: AccountId,
    tokens: &BTreeSet<String>,
) -> Result<Vec<TokenStat>, MailError> {
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; tokens.len()].join(", ");
    let sql = format!(
        "SELECT ham_count, spam_count FROM junk_tokens \
         WHERE account_id = ? AND token IN ({placeholders})"
    );
    let mut query = sqlx::query_as::<_, (i64, i64)>(sqlx::AssertSqlSafe(sql)).bind(account.value());
    for token in tokens {
        query = query.bind(token);
    }
    let rows = query
        .fetch_all(&mut *conn)
        .await
        .map_err(|error| map_sqlx("junk_load_tokens", error))?;
    Ok(rows
        .into_iter()
        .map(|(ham, spam)| TokenStat {
            ham: clamp_u32(ham),
            spam: clamp_u32(spam),
        })
        .collect())
}

/// Scores a token set for `account`: neutral when the corpus is empty, else the
/// classifier's probability over the tokens' stored counts.
pub(crate) async fn score_on(
    conn: &mut SqliteConnection,
    account: AccountId,
    tokens: &BTreeSet<String>,
) -> Result<f64, MailError> {
    let corpus = load_corpus(conn, account).await?;
    if corpus.is_empty() {
        return Ok(junk::NEUTRAL_SCORE);
    }
    let stats = load_present_stats(conn, account, tokens).await?;
    Ok(junk::score(&stats, corpus))
}

/// Scores freshly ingested inbound envelopes and persists each score, returning
/// the ids that should be auto-filed as junk (score at or above the threshold
/// and, when the whitelist is on, not from a known contact).
///
/// A no-op — empty result — when junk control is disabled. The caller gates
/// this on the mailbox being the Inbox, so only genuinely inbound mail is
/// scored (never Sent/Drafts). Scoring uses the envelope's subject and sender
/// (no body is available at envelope-only ingest); the "run junk controls"
/// command rescs with body + server headers when the raw is present.
pub(crate) async fn score_new_inbound(
    conn: &mut SqliteConnection,
    account: AccountId,
    messages: &[(i64, &RemoteEnvelope)],
) -> Result<Vec<MessageId>, MailError> {
    let settings = load_settings(conn).await?;
    if !settings.enabled || messages.is_empty() {
        return Ok(Vec::new());
    }
    let mut candidates = Vec::new();
    for &(message_id, envelope) in messages {
        let tokens = message_tokens(envelope.subject.as_deref(), &envelope.from, "");
        let score = score_on(conn, account, &tokens).await?;
        set_score_on(conn, message_id, score).await?;
        if score < settings.threshold {
            continue;
        }
        if settings.whitelist_contacts
            && let Some(sender) = envelope.from.first()
            && is_whitelisted(conn, sender.email.as_str()).await?
        {
            continue;
        }
        candidates.push(MessageId::new(message_id));
    }
    Ok(candidates)
}

/// Trains `account` on one message: bumps the corpus total for the class and,
/// per distinct token, its ham or spam count (upserts).
pub(crate) async fn train_on(
    conn: &mut SqliteConnection,
    account: AccountId,
    tokens: &BTreeSet<String>,
    spam: bool,
) -> Result<(), MailError> {
    let (ham_delta, spam_delta): (i64, i64) = if spam { (0, 1) } else { (1, 0) };
    sqlx::query(
        "INSERT INTO junk_corpus (account_id, ham_messages, spam_messages) VALUES (?, ?, ?)
         ON CONFLICT(account_id) DO UPDATE SET
             ham_messages = ham_messages + excluded.ham_messages,
             spam_messages = spam_messages + excluded.spam_messages",
    )
    .bind(account.value())
    .bind(ham_delta)
    .bind(spam_delta)
    .execute(&mut *conn)
    .await
    .map_err(|error| map_sqlx("junk_train_corpus", error))?;
    for token in tokens {
        sqlx::query(
            "INSERT INTO junk_tokens (account_id, token, ham_count, spam_count) VALUES (?, ?, ?, ?)
             ON CONFLICT(account_id, token) DO UPDATE SET
                 ham_count = ham_count + excluded.ham_count,
                 spam_count = spam_count + excluded.spam_count",
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

/// Persists a message's computed junk score (a probability in `0.0..=1.0`).
pub(crate) async fn set_score_on(
    conn: &mut SqliteConnection,
    message_id: i64,
    score: f64,
) -> Result<(), MailError> {
    sqlx::query("UPDATE messages SET junk_score = ? WHERE id = ?")
        .bind(score)
        .bind(message_id)
        .execute(&mut *conn)
        .await
        .map_err(|error| map_sqlx("junk_set_score", error))?;
    Ok(())
}

/// Whether `email` appears in the global address book — the contact whitelist.
///
/// The address book (track 4.1) is global rather than per-account, so the
/// whitelist is too: a sender saved as a contact under any account is trusted
/// and never auto-filed as junk. Matching is on `contact_emails.email`, which
/// carries `COLLATE NOCASE`, so the comparison is case-insensitive and
/// index-served (`idx_contact_emails_email`). A query error fails safe to "not
/// whitelisted" (logged) so a lookup hiccup never silently disables junk
/// routing.
pub(crate) async fn is_whitelisted(
    conn: &mut SqliteConnection,
    email: &str,
) -> Result<bool, MailError> {
    let query =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM contact_emails WHERE email = ? LIMIT 1")
            .bind(email);
    match query.fetch_optional(&mut *conn).await {
        Ok(found) => Ok(found.is_some()),
        Err(error) => {
            tracing::warn!(error = %error, "junk whitelist lookup failed; treating sender as not whitelisted");
            Ok(false)
        }
    }
}

/// Narrows a stored count to `u32`, saturating (counts are always non-negative
/// and far below `u32::MAX` in practice).
fn clamp_u32(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// A single-connection in-memory pool so a created table stays visible.
    async fn pool() -> sqlx::SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn whitelist_is_inactive_without_a_matching_contact() {
        let pool = pool().await;
        sqlx::query("CREATE TABLE contact_emails (email TEXT NOT NULL COLLATE NOCASE)")
            .execute(&pool)
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        assert!(!is_whitelisted(&mut conn, "grace@example.org").await.unwrap());
    }

    #[tokio::test]
    async fn whitelist_matches_a_known_contact_case_insensitively() {
        let pool = pool().await;
        // The real address book stores addresses in `contact_emails` with a
        // NOCASE collation; mirror just that column here.
        sqlx::query("CREATE TABLE contact_emails (email TEXT NOT NULL COLLATE NOCASE)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO contact_emails (email) VALUES ('Grace@Example.org')")
            .execute(&pool)
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        // The address book is global: a saved contact address is whitelisted,
        // matched case-insensitively via the column's NOCASE collation.
        assert!(is_whitelisted(&mut conn, "grace@example.org").await.unwrap());
        // A stranger is not.
        assert!(!is_whitelisted(&mut conn, "spam@example.org").await.unwrap());
    }
}
