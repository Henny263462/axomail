//! Transactional keyword persistence used by the sync ingest path.
//!
//! When a sync pass reports a message's IMAP keywords, ingest projects them
//! into `message_tags`, auto-creating an account tag for any keyword not seen
//! before (with a derived display name and no color). These helpers run on the
//! ingest transaction's connection, never on the pool, so they land atomically
//! with the envelope/flag write in the same pass.
//!
//! Two shapes are exposed: [`add_message_keywords`] for a freshly inserted
//! message (pure inserts, no prior rows to clear) and
//! [`reconcile_message_keywords`] for a flag update, which replaces the
//! message's keyword tags with the server-reported set — the server is
//! authoritative for keywords, exactly as it is for the five system flags.

use std::collections::BTreeSet;

use axomail_core::{AccountId, Keyword, MailError};
use sqlx::SqliteConnection;

use crate::error_map::map_sqlx;

/// Adds the given keywords to a message, auto-creating account tags as needed.
///
/// Intended for a newly inserted message: it only inserts associations (and is
/// a no-op for an empty keyword set), so it never issues a delete.
pub(crate) async fn add_message_keywords(
    conn: &mut SqliteConnection,
    account: AccountId,
    message_id: i64,
    keywords: &BTreeSet<Keyword>,
) -> Result<(), MailError> {
    for keyword in keywords {
        let tag_id = ensure_tag(conn, account, keyword).await?;
        link_message_tag(conn, message_id, tag_id).await?;
    }
    Ok(())
}

/// Replaces a message's keyword tags with exactly `keywords`.
///
/// Used on a flag update, where the server reports the message's complete
/// current keyword set: any tag no longer present is unlinked and any new one
/// is linked (auto-creating its account tag). All tags are keyword-backed, so
/// clearing and re-linking is the correct server-authoritative reconciliation.
pub(crate) async fn reconcile_message_keywords(
    conn: &mut SqliteConnection,
    account: AccountId,
    message_id: i64,
    keywords: &BTreeSet<Keyword>,
) -> Result<(), MailError> {
    sqlx::query("DELETE FROM message_tags WHERE message_id = ?")
        .bind(message_id)
        .execute(&mut *conn)
        .await
        .map_err(|error| map_sqlx("reconcile_message_keywords", error))?;
    add_message_keywords(conn, account, message_id, keywords).await
}

/// Returns the id of the account's tag for `keyword`, creating it (with a
/// derived display name and no color) if it does not yet exist.
async fn ensure_tag(
    conn: &mut SqliteConnection,
    account: AccountId,
    keyword: &Keyword,
) -> Result<i64, MailError> {
    // The no-op `DO UPDATE` makes the upsert return the existing row's id on
    // conflict without disturbing its display name, color, or stored case.
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO tags (account_id, keyword, display_name, color, sort_order)
         VALUES (?, ?, ?, NULL, 0)
         ON CONFLICT (account_id, keyword) DO UPDATE SET display_name = tags.display_name
         RETURNING id",
    )
    .bind(account.value())
    .bind(keyword.as_str())
    .bind(derive_display_name(keyword))
    .fetch_one(&mut *conn)
    .await
    .map_err(|error| map_sqlx("ensure_tag", error))?;
    Ok(id)
}

/// Links a message to a tag, ignoring a duplicate association.
async fn link_message_tag(
    conn: &mut SqliteConnection,
    message_id: i64,
    tag_id: i64,
) -> Result<(), MailError> {
    sqlx::query(
        "INSERT INTO message_tags (message_id, tag_id) VALUES (?, ?)
         ON CONFLICT DO NOTHING",
    )
    .bind(message_id)
    .bind(tag_id)
    .execute(&mut *conn)
    .await
    .map_err(|error| map_sqlx("link_message_tag", error))?;
    Ok(())
}

/// Derives a display name for an auto-created inbound tag: the keyword with a
/// single leading `$` stripped (so `$Junk` shows as "Junk"), or the keyword
/// verbatim when stripping would leave it empty.
fn derive_display_name(keyword: &Keyword) -> String {
    let text = keyword.as_str();
    match text.strip_prefix('$') {
        Some(rest) if !rest.is_empty() => rest.to_owned(),
        _ => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn display_name_strips_leading_dollar() {
        assert_eq!(
            derive_display_name(&Keyword::parse("$Junk").unwrap()),
            "Junk"
        );
        assert_eq!(
            derive_display_name(&Keyword::parse("Work").unwrap()),
            "Work"
        );
        assert_eq!(derive_display_name(&Keyword::parse("$").unwrap()), "$");
    }
}
