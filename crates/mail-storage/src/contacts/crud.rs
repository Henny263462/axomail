//! Contact create/update/delete/read/list methods on [`ContactRepo`].

use std::collections::HashMap;

use axomail_core::MailError;

use super::{
    CONTACT_COLUMNS, Contact, ContactEmail, ContactId, ContactRepo, ContactRow, ContactUpdate,
    EmailRow, LIST_CAP, NewContact, columns_prefixed, escape_like, fallback_name, normalize_emails,
    write_emails,
};
use crate::error_map::map_sqlx;

impl ContactRepo<'_> {
    /// Inserts a contact and its addresses, returning it with its id. The
    /// address set is normalized (deduplicated case-insensitively, exactly one
    /// primary) and mirrored into `primary_email`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn create(&self, new: &NewContact) -> Result<Contact, MailError> {
        let (emails, primary) = normalize_emails(&new.emails);
        let display_name = fallback_name(&new.display_name, primary.as_deref());

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("create_contact", error))?;

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (display_name, primary_email, note)
             VALUES (?, ?, ?)
             RETURNING id",
        )
        .bind(&display_name)
        .bind(primary.as_deref())
        .bind(new.note.trim())
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| map_sqlx("create_contact", error))?;

        write_emails(&mut tx, id, &emails).await?;

        tx.commit()
            .await
            .map_err(|error| map_sqlx("create_contact", error))?;

        self.get(ContactId::new(id))
            .await?
            .ok_or_else(|| MailError::Database {
                message: "create_contact: inserted row vanished".to_owned(),
            })
    }

    /// Fully replaces a contact's name, note, and address set. Returns the
    /// updated contact, or `None` if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn update(
        &self,
        id: ContactId,
        update: &ContactUpdate,
    ) -> Result<Option<Contact>, MailError> {
        let (emails, primary) = normalize_emails(&update.emails);
        let display_name = fallback_name(&update.display_name, primary.as_deref());

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("update_contact", error))?;

        let affected = sqlx::query(
            "UPDATE contacts
             SET display_name = ?, primary_email = ?, note = ?, updated_at_unix = unixepoch()
             WHERE id = ?",
        )
        .bind(&display_name)
        .bind(primary.as_deref())
        .bind(update.note.trim())
        .bind(id.value())
        .execute(&mut *tx)
        .await
        .map_err(|error| map_sqlx("update_contact", error))?
        .rows_affected();

        if affected == 0 {
            return Ok(None);
        }

        sqlx::query("DELETE FROM contact_emails WHERE contact_id = ?")
            .bind(id.value())
            .execute(&mut *tx)
            .await
            .map_err(|error| map_sqlx("update_contact", error))?;
        write_emails(&mut tx, id.value(), &emails).await?;

        tx.commit()
            .await
            .map_err(|error| map_sqlx("update_contact", error))?;

        self.get(id).await
    }

    /// Deletes a contact, cascading to its addresses and group memberships.
    /// Returns `true` if a row was removed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete(&self, id: ContactId) -> Result<bool, MailError> {
        let result = sqlx::query("DELETE FROM contacts WHERE id = ?")
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("delete_contact", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Fetches a contact with its addresses by id, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn get(&self, id: ContactId) -> Result<Option<Contact>, MailError> {
        let row = sqlx::query_as::<_, ContactRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {CONTACT_COLUMNS} FROM contacts WHERE id = ?"
        )))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_contact", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let mut emails = self.emails_for_contacts(&[id.value()]).await?;
        Ok(Some(row.into_contact(
            emails.remove(&id.value()).unwrap_or_default(),
        )?))
    }

    /// Lists contacts alphabetically, optionally filtered by a name/email
    /// substring (the address-book search box). A blank query returns the whole
    /// book, capped at [`LIST_CAP`].
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn list(&self, query: &str, limit: u32) -> Result<Vec<Contact>, MailError> {
        let limit = i64::from(limit.clamp(1, LIST_CAP));
        let trimmed = query.trim();
        let rows = if trimmed.is_empty() {
            sqlx::query_as::<_, ContactRow>(sqlx::AssertSqlSafe(format!(
                "SELECT {CONTACT_COLUMNS} FROM contacts
                 ORDER BY display_name COLLATE NOCASE, id LIMIT ?"
            )))
            .bind(limit)
            .fetch_all(self.pool)
            .await
        } else {
            let pattern = format!("%{}%", escape_like(trimmed));
            sqlx::query_as::<_, ContactRow>(sqlx::AssertSqlSafe(format!(
                "SELECT DISTINCT {} FROM contacts c
                 LEFT JOIN contact_emails ce ON ce.contact_id = c.id
                 WHERE c.display_name LIKE ?1 ESCAPE '\\' OR ce.email LIKE ?1 ESCAPE '\\'
                 ORDER BY c.display_name COLLATE NOCASE, c.id LIMIT ?2",
                columns_prefixed("c")
            )))
            .bind(pattern)
            .bind(limit)
            .fetch_all(self.pool)
            .await
        }
        .map_err(|error| map_sqlx("list_contacts", error))?;
        self.attach_emails(rows).await
    }

    /// Loads and attaches every contact's addresses to a batch of rows,
    /// returning full [`Contact`]s in the rows' order.
    pub(super) async fn attach_emails(
        &self,
        rows: Vec<ContactRow>,
    ) -> Result<Vec<Contact>, MailError> {
        let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
        let mut emails = self.emails_for_contacts(&ids).await?;
        rows.into_iter()
            .map(|row| {
                let id = row.id;
                row.into_contact(emails.remove(&id).unwrap_or_default())
            })
            .collect()
    }

    /// Returns the addresses of a set of contacts in one query, grouped by
    /// contact id (primary first). An empty id slice touches no database.
    async fn emails_for_contacts(
        &self,
        contact_ids: &[i64],
    ) -> Result<HashMap<i64, Vec<ContactEmail>>, MailError> {
        let mut grouped: HashMap<i64, Vec<ContactEmail>> = HashMap::new();
        if contact_ids.is_empty() {
            return Ok(grouped);
        }
        let placeholders = vec!["?"; contact_ids.len()].join(", ");
        let sql = format!(
            "SELECT contact_id, email, label, is_primary FROM contact_emails
             WHERE contact_id IN ({placeholders})
             ORDER BY contact_id, is_primary DESC, id"
        );
        let mut query = sqlx::query_as::<_, EmailRow>(sqlx::AssertSqlSafe(sql));
        for id in contact_ids {
            query = query.bind(id);
        }
        let rows = query
            .fetch_all(self.pool)
            .await
            .map_err(|error| map_sqlx("emails_for_contacts", error))?;
        for row in rows {
            grouped
                .entry(row.contact_id)
                .or_default()
                .push(row.into_email()?);
        }
        Ok(grouped)
    }
}
