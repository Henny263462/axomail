//! Loads a message's [`FilterMessageFacts`] from storage for the evaluator.
//!
//! Address fields are rendered to lowercase `name email` strings so a condition
//! can match either the display name or the address. The body comes from the
//! FTS content table (the full body once the message has been opened or
//! downloaded, otherwise the short preview), which is the best plain-text the
//! store has locally — a documented limitation of matching `body` on a message
//! whose full text has not been fetched yet.

use axomail_core::{MailError, MessageId};

use super::FilterRepo;
use super::eval::FilterMessageFacts;
use crate::error_map::map_sqlx;

/// One recipient row (kind + rendered address) for a message.
#[derive(sqlx::FromRow)]
struct RecipientRow {
    kind: String,
    display_name: Option<String>,
    email: String,
}

/// The scalar message columns the evaluator needs.
#[derive(sqlx::FromRow)]
struct FactsRow {
    subject: Option<String>,
    size_bytes: Option<i64>,
    date_unix: Option<i64>,
    preview: Option<String>,
}

impl FilterRepo<'_> {
    /// Builds the evaluator facts for a message, or `None` if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn facts_for_message(
        &self,
        message: MessageId,
    ) -> Result<Option<FilterMessageFacts>, MailError> {
        let Some(row) = sqlx::query_as::<_, FactsRow>(
            "SELECT subject, size_bytes, date_unix, preview FROM messages WHERE id = ?",
        )
        .bind(message.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("filter_facts_message", error))?
        else {
            return Ok(None);
        };

        let recipients = sqlx::query_as::<_, RecipientRow>(
            "SELECT kind, display_name, email FROM message_recipients \
             WHERE message_id = ? ORDER BY kind, position, id",
        )
        .bind(message.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("filter_facts_recipients", error))?;

        let (mut from, mut to, mut cc) = (Vec::new(), Vec::new(), Vec::new());
        for recipient in recipients {
            let rendered = render_address(recipient.display_name.as_deref(), &recipient.email);
            match recipient.kind.as_str() {
                "from" => from.push(rendered),
                "to" => to.push(rendered),
                "cc" => cc.push(rendered),
                _ => {} // bcc is not a filterable field
            }
        }

        let body: Option<String> =
            sqlx::query_scalar("SELECT body FROM message_search_content WHERE message_id = ?")
                .bind(message.value())
                .fetch_optional(self.pool)
                .await
                .map_err(|error| map_sqlx("filter_facts_body", error))?;
        let body = body
            .filter(|text| !text.trim().is_empty())
            .or(row.preview)
            .unwrap_or_default()
            .to_lowercase();

        let tags = sqlx::query_as::<_, (String, String)>(
            "SELECT t.keyword, t.display_name FROM tags t \
             JOIN message_tags mt ON mt.tag_id = t.id WHERE mt.message_id = ?",
        )
        .bind(message.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("filter_facts_tags", error))?
        .into_iter()
        .flat_map(|(keyword, display)| [keyword.to_lowercase(), display.to_lowercase()])
        .collect();

        let has_attachment: bool = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM attachments WHERE message_id = ?)",
        )
        .bind(message.value())
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("filter_facts_attachment", error))?
            != 0;

        Ok(Some(FilterMessageFacts {
            subject: row.subject.unwrap_or_default().to_lowercase(),
            from,
            to,
            cc,
            body,
            tags,
            has_attachment,
            size: row.size_bytes.and_then(|s| u64::try_from(s).ok()),
            date: row.date_unix,
        }))
    }
}

/// Renders an address as a lowercased `name email` (or just the email) for
/// case-insensitive matching against either part.
fn render_address(display_name: Option<&str>, email: &str) -> String {
    match display_name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => format!("{name} {email}").to_lowercase(),
        None => email.to_lowercase(),
    }
}
