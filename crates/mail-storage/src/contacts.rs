//! Contact repository: the global address book (contacts + their addresses),
//! auto-collection of sent recipients, prefix autocomplete, and groups.
//!
//! Contacts are global (not per-account): a person is the same regardless of
//! which account mails them. Each contact keeps a denormalized `primary_email`
//! for cheap list rendering; the authoritative address set lives in
//! `contact_emails`, and the repository keeps the two in sync on every write so
//! a caller can never leave them inconsistent.
//!
//! The surface is split across sibling modules to stay within the file-size
//! gate: [`crud`] holds create/update/delete/read/list, [`search`] holds
//! autocomplete + auto-collection, and [`groups`] holds the mailing-list
//! methods. This root owns the shared struct, row types, and normalization
//! helpers they all use.

mod crud;
mod groups;
mod search;
mod types;

use axomail_core::{EmailAddress, MailError};
use sqlx::{SqliteConnection, SqlitePool};

pub use types::{
    Contact, ContactEmail, ContactGroup, ContactGroupId, ContactId, ContactMatch, ContactUpdate,
    NewContact, NewContactEmail,
};

use crate::error_map::map_sqlx;

/// Columns selected when reading a contact, in [`ContactRow`] order.
const CONTACT_COLUMNS: &str =
    "id, display_name, primary_email, note, created_at_unix, updated_at_unix";

/// Cap on how many contacts the unfiltered address-book listing returns; a
/// larger book is reached through the search box.
const LIST_CAP: u32 = 1000;

/// Repository over the `contacts`, `contact_emails`, `contact_groups`, and
/// `contact_group_members` tables.
#[derive(Debug)]
pub struct ContactRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ContactRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }
}

/// Builds the [`CONTACT_COLUMNS`] list qualified with a table alias.
fn columns_prefixed(alias: &str) -> String {
    CONTACT_COLUMNS
        .split(", ")
        .map(|col| format!("{alias}.{col}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A deduplicated, primary-resolved address ready for insertion.
struct NormalizedEmail {
    email: String,
    label: String,
    is_primary: bool,
}

/// Deduplicates addresses case-insensitively (keeping the first occurrence),
/// then ensures exactly one is primary: the caller's first `is_primary`, else
/// the first address. Returns the ordered rows and the chosen primary address.
fn normalize_emails(input: &[NewContactEmail]) -> (Vec<NormalizedEmail>, Option<String>) {
    let mut seen: Vec<String> = Vec::new();
    let mut rows: Vec<NormalizedEmail> = Vec::new();
    for candidate in input {
        let address = candidate.email.as_str();
        if seen.iter().any(|e| e.eq_ignore_ascii_case(address)) {
            continue;
        }
        seen.push(address.to_owned());
        rows.push(NormalizedEmail {
            email: address.to_owned(),
            label: candidate.label.trim().to_owned(),
            is_primary: candidate.is_primary,
        });
    }
    if rows.is_empty() {
        return (rows, None);
    }
    let primary_index = rows.iter().position(|row| row.is_primary).unwrap_or(0);
    for (index, row) in rows.iter_mut().enumerate() {
        row.is_primary = index == primary_index;
    }
    let primary = rows[primary_index].email.clone();
    (rows, Some(primary))
}

/// Returns a non-blank display name: the trimmed input, or the local part of
/// the primary address, or the address itself, or a placeholder.
fn fallback_name(display_name: &str, primary: Option<&str>) -> String {
    let trimmed = display_name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_owned();
    }
    match primary {
        Some(email) => {
            let local = email.split('@').next().unwrap_or(email);
            if local.is_empty() {
                email.to_owned()
            } else {
                local.to_owned()
            }
        }
        None => "(no name)".to_owned(),
    }
}

/// Escapes the `LIKE` wildcards `%`, `_`, and the escape char `\` so a
/// user-typed query is matched literally (paired with `ESCAPE '\'`).
fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Inserts a contact's normalized addresses within a transaction.
async fn write_emails(
    tx: &mut SqliteConnection,
    contact_id: i64,
    emails: &[NormalizedEmail],
) -> Result<(), MailError> {
    for row in emails {
        sqlx::query(
            "INSERT INTO contact_emails (contact_id, email, label, is_primary)
             VALUES (?, ?, ?, ?)",
        )
        .bind(contact_id)
        .bind(&row.email)
        .bind(&row.label)
        .bind(row.is_primary)
        .execute(&mut *tx)
        .await
        .map_err(|error| map_sqlx("write_contact_emails", error))?;
    }
    Ok(())
}

/// Raw `contacts` row.
#[derive(sqlx::FromRow)]
struct ContactRow {
    id: i64,
    display_name: String,
    primary_email: Option<String>,
    note: String,
    created_at_unix: i64,
    updated_at_unix: i64,
}

impl ContactRow {
    fn into_contact(self, emails: Vec<ContactEmail>) -> Result<Contact, MailError> {
        let primary_email = self
            .primary_email
            .map(|value| EmailAddress::parse(&value))
            .transpose()?;
        Ok(Contact {
            id: ContactId::new(self.id),
            display_name: self.display_name,
            primary_email,
            note: self.note,
            emails,
            created_at_unix: self.created_at_unix,
            updated_at_unix: self.updated_at_unix,
        })
    }
}

/// Raw `contact_emails` row.
#[derive(sqlx::FromRow)]
struct EmailRow {
    contact_id: i64,
    email: String,
    label: String,
    is_primary: bool,
}

impl EmailRow {
    fn into_email(self) -> Result<ContactEmail, MailError> {
        Ok(ContactEmail {
            email: EmailAddress::parse(&self.email)?,
            label: self.label,
            is_primary: self.is_primary,
        })
    }
}

/// Raw autocomplete match row.
#[derive(sqlx::FromRow)]
struct MatchRow {
    contact_id: i64,
    display_name: String,
    email: String,
}

impl MatchRow {
    fn into_match(self) -> Result<ContactMatch, MailError> {
        Ok(ContactMatch {
            contact_id: ContactId::new(self.contact_id),
            display_name: self.display_name,
            email: EmailAddress::parse(&self.email)?,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn normalize_dedups_and_picks_a_single_primary() {
        let email = |addr: &str, primary: bool| NewContactEmail {
            email: EmailAddress::parse(addr).unwrap(),
            label: String::new(),
            is_primary: primary,
        };
        // Duplicate (case-insensitive) is dropped; the marked primary wins.
        let (rows, primary) = normalize_emails(&[
            email("a@example.org", false),
            email("A@example.org", false),
            email("b@example.org", true),
        ]);
        assert_eq!(rows.len(), 2);
        assert_eq!(primary.as_deref(), Some("b@example.org"));
        assert!(!rows[0].is_primary);
        assert!(rows[1].is_primary);
    }

    #[test]
    fn normalize_promotes_first_when_none_marked() {
        let (rows, primary) = normalize_emails(&[NewContactEmail::plain(
            EmailAddress::parse("only@example.org").unwrap(),
        )]);
        assert!(rows[0].is_primary);
        assert_eq!(primary.as_deref(), Some("only@example.org"));
    }

    #[test]
    fn fallback_name_uses_local_part_when_blank() {
        assert_eq!(fallback_name("  ", Some("ada@example.org")), "ada");
        assert_eq!(fallback_name("Ada", Some("ada@example.org")), "Ada");
        assert_eq!(fallback_name("", None), "(no name)");
    }

    #[test]
    fn escape_like_escapes_wildcards() {
        assert_eq!(escape_like("50%_\\x"), "50\\%\\_\\\\x");
        assert_eq!(escape_like("plain"), "plain");
    }
}
