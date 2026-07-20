//! Autocomplete search, email lookup, and sent-recipient auto-collection on
//! [`ContactRepo`].

use axomail_core::{EmailAddress, MailError};

use super::{
    Contact, ContactId, ContactMatch, ContactRepo, MatchRow, NewContact, NewContactEmail,
    escape_like,
};
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
    /// The address's own case is preserved; the case-insensitive uniqueness of
    /// `contact_emails.email` makes the existence check case-agnostic.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn collect_address(
        &self,
        email: &EmailAddress,
        display_name: &str,
    ) -> Result<bool, MailError> {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM contact_emails WHERE email = ? LIMIT 1")
                .bind(email.as_str())
                .fetch_optional(self.pool)
                .await
                .map_err(|error| map_sqlx("collect_address", error))?;
        if exists.is_some() {
            return Ok(false);
        }
        self.create(&NewContact {
            display_name: display_name.to_owned(),
            emails: vec![NewContactEmail {
                email: email.clone(),
                label: String::new(),
                is_primary: true,
            }],
            note: String::new(),
        })
        .await?;
        Ok(true)
    }
}
