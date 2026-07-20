//! Autocomplete search, email lookup, and sent-recipient auto-collection on
//! [`ContactRepo`].

use axomail_core::{EmailAddress, MailError};

use super::{Contact, ContactId, ContactMatch, ContactRepo, MatchRow, escape_like, fallback_name};
use crate::error_map::map_sqlx;

impl ContactRepo<'_> {
    /// Prefix autocomplete over contact names and addresses, for the composer
    /// recipient fields. Each returned match inserts one specific address. An
    /// empty query returns nothing.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn search_recipients(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<ContactMatch>, MailError> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let escaped = escape_like(trimmed);
        // Prefix on the address, prefix on the name, and a word-boundary prefix
        // inside the name ("lov" matching "Ada Lovelace"), so typing any name
        // token surfaces the contact.
        let prefix = format!("{escaped}%");
        let word = format!("% {escaped}%");
        let rows = sqlx::query_as::<_, MatchRow>(
            "SELECT c.id AS contact_id, c.display_name AS display_name, ce.email AS email
             FROM contact_emails ce
             JOIN contacts c ON c.id = ce.contact_id
             WHERE ce.email LIKE ?1 ESCAPE '\\'
                OR c.display_name LIKE ?1 ESCAPE '\\'
                OR c.display_name LIKE ?2 ESCAPE '\\'
             ORDER BY ce.is_primary DESC, c.display_name COLLATE NOCASE, ce.id
             LIMIT ?3",
        )
        .bind(prefix)
        .bind(word)
        .bind(i64::from(limit.max(1)))
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("search_recipients", error))?;
        rows.into_iter().map(MatchRow::into_match).collect()
    }

    /// Finds the contact owning `email`, if any (case-insensitive).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn find_by_email(&self, email: &EmailAddress) -> Result<Option<Contact>, MailError> {
        let id: Option<i64> =
            sqlx::query_scalar("SELECT contact_id FROM contact_emails WHERE email = ? LIMIT 1")
                .bind(email.as_str())
                .fetch_optional(self.pool)
                .await
                .map_err(|error| map_sqlx("find_contact_by_email", error))?;
        match id {
            Some(id) => self.get(ContactId::new(id)).await,
            None => Ok(None),
        }
    }

    /// Auto-collects a sent recipient: if a contact already owns `email`, does
    /// nothing; otherwise creates a name-only contact carrying it as its
    /// primary address. Returns `true` when a new contact was created.
    ///
    /// Race-safe: the address is inserted atomically, and the global unique
    /// index on `contact_emails.email` (case-insensitive) means a concurrent
    /// collect that inserted first trips a unique violation, which is treated as
    /// "already collected" (`Ok(false)`). This closes the check-then-insert
    /// window where two concurrent sends to a new address each created a
    /// duplicate contact.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn collect_address(
        &self,
        email: &EmailAddress,
        display_name: &str,
    ) -> Result<bool, MailError> {
        // Fast path: already known (case-insensitive via the column collation).
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM contact_emails WHERE email = ? LIMIT 1")
                .bind(email.as_str())
                .fetch_optional(self.pool)
                .await
                .map_err(|error| map_sqlx("collect_address", error))?;
        if exists.is_some() {
            return Ok(false);
        }

        let display = fallback_name(display_name, Some(email.as_str()));
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("collect_address", error))?;
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (display_name, primary_email, note) VALUES (?, ?, '') RETURNING id",
        )
        .bind(&display)
        .bind(email.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| map_sqlx("collect_address", error))?;

        // The email carries the global unique index; a concurrent collect that
        // won the race trips a unique violation here.
        let inserted = sqlx::query(
            "INSERT INTO contact_emails (contact_id, email, label, is_primary) VALUES (?, ?, '', 1)",
        )
        .bind(contact_id)
        .bind(email.as_str())
        .execute(&mut *tx)
        .await;
        match inserted {
            Ok(_) => {
                tx.commit()
                    .await
                    .map_err(|error| map_sqlx("collect_address", error))?;
                Ok(true)
            }
            Err(error) if is_unique_violation(&error) => {
                // Lost the race: another collect already created this contact.
                // Roll back our half-built one and report "already collected".
                let _ = tx.rollback().await;
                Ok(false)
            }
            Err(error) => Err(map_sqlx("collect_address", error)),
        }
    }
}

/// Whether a query error is a unique-constraint violation (a lost auto-collect
/// race), as opposed to any other database failure.
fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
}
