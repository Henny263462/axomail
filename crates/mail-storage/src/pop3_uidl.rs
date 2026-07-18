//! POP3 UIDL de-duplication repository.
//!
//! POP3 has no `UIDVALIDITY` and no server-side cursor, so the only stable
//! way to tell which messages are already downloaded is to remember every
//! `UIDL` ever ingested for an account. That memory lives in the `pop3_uidl`
//! table (see migration `0003`), keyed uniquely by `(account_id, uidl)`, and
//! deliberately outlives the message rows themselves: its `message_id`
//! foreign key is `ON DELETE SET NULL`, so a locally deleted message never
//! reappears on the next sync.
//!
//! The composition layer uses this repository in two steps around each POP3
//! sync pass:
//!
//! 1. [`Pop3UidlRepo::record_ingested`] copies the `UIDL`s of already-stored
//!    POP3 messages into the table. Because it reads from committed message
//!    rows, a crash between a sync pass and its ingest can never record a
//!    `UIDL` for a message that was not durably stored — the record simply
//!    lags one pass, which is the crash-safe direction.
//! 2. [`Pop3UidlRepo::known_uidls`] returns the full known set, which is
//!    handed to `Pop3Provider::set_known_uidls` so the adapter skips every
//!    message the engine already has.

use std::collections::HashSet;

use axomail_core::{AccountId, MailError};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;

/// Repository over the `pop3_uidl` de-duplication table.
#[derive(Debug)]
pub struct Pop3UidlRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> Pop3UidlRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Returns every `UIDL` already known for `account`.
    ///
    /// The set is the union of everything ever recorded by
    /// [`record_ingested`](Self::record_ingested); it is fed to the POP3
    /// adapter before a sync so already-downloaded messages are skipped.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn known_uidls(&self, account: AccountId) -> Result<HashSet<String>, MailError> {
        let rows: Vec<String> =
            sqlx::query_scalar("SELECT uidl FROM pop3_uidl WHERE account_id = ?")
                .bind(account.value())
                .fetch_all(self.pool)
                .await
                .map_err(|error| map_sqlx("known_pop3_uidls", error))?;
        Ok(rows.into_iter().collect())
    }

    /// Records the `UIDL` of every ingested POP3 message of `account` that is
    /// not yet in the table, linking each to its message row. Returns the
    /// number of newly recorded `UIDL`s.
    ///
    /// Reading from committed `messages` rows (never from an in-flight sync
    /// outcome) is what makes recording crash-safe: only durably stored
    /// messages contribute, so a `UIDL` is never remembered for a message
    /// that a crash left un-ingested. Existing rows are left untouched, so a
    /// `UIDL` whose message was later deleted keeps its de-duplication entry.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn record_ingested(&self, account: AccountId) -> Result<u64, MailError> {
        let result = sqlx::query(
            "INSERT INTO pop3_uidl (account_id, uidl, message_id)
             SELECT mb.account_id, m.remote_uidl, m.id
             FROM messages m
             JOIN mailboxes mb ON mb.id = m.mailbox_id
             WHERE mb.account_id = ?
               AND m.remote_kind = 'pop3'
               AND m.remote_uidl IS NOT NULL
             ON CONFLICT (account_id, uidl) DO NOTHING",
        )
        .bind(account.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("record_pop3_uidls", error))?;
        Ok(result.rows_affected())
    }
}
