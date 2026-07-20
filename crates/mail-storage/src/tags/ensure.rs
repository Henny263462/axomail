//! Idempotent tag resolution for machine-applied tags (filter `tag` and
//! `mark_junk` actions): find an account's tag by display name or keyword, or
//! create one, deriving a unique keyword atom from the name.
//!
//! Kept in a sibling module so `tags.rs` stays under the file-size gate. Uses
//! only the public [`TagRepo`] surface, so it never duplicates SQL.

use axomail_core::{AccountId, Keyword, MailError};

use super::TagRepo;
use super::types::{NewTag, Tag};

impl TagRepo<'_> {
    /// Returns the account's tag matching `name` (by display name or keyword,
    /// case-insensitively), creating a colorless one appended to the list when
    /// none exists. Used to resolve a filter's `tag`/`mark_junk` action target.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] for a blank name and
    /// [`MailError::Database`] on any query failure.
    pub async fn ensure_by_name(&self, account: AccountId, name: &str) -> Result<Tag, MailError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(MailError::Configuration {
                message: "a tag name must not be empty".to_owned(),
            });
        }
        let existing = self.list(account).await?;
        if let Some(tag) = existing.into_iter().find(|tag| {
            tag.display_name.eq_ignore_ascii_case(trimmed)
                || tag.keyword.as_str().eq_ignore_ascii_case(trimmed)
        }) {
            return Ok(tag);
        }
        let keyword = self.unique_keyword(account, trimmed).await?;
        let sort_order = i64::try_from(self.list(account).await?.len()).unwrap_or(0);
        self.create(&NewTag {
            account,
            keyword,
            display_name: trimmed.to_owned(),
            color: None,
            sort_order,
        })
        .await
    }

    /// Derives a keyword atom from `name` that is free within `account`,
    /// appending a numeric suffix on collision. Falls back to `label` when the
    /// name has no atom-safe characters.
    async fn unique_keyword(&self, account: AccountId, name: &str) -> Result<Keyword, MailError> {
        let base = match Keyword::parse(&keyword_atom(name)) {
            Ok(keyword) => keyword.as_str().to_owned(),
            Err(_) => "label".to_owned(),
        };
        let mut candidate = base.clone();
        let mut suffix = 1u32;
        loop {
            let keyword = Keyword::parse(&candidate)?;
            if self.find_by_keyword(account, &keyword).await?.is_none() {
                return Ok(keyword);
            }
            suffix += 1;
            candidate = format!("{base}{suffix}");
        }
    }
}

/// Keeps only the RFC 3501 atom characters of `name` (dropping spaces and
/// atom-special characters), the basis for a derived keyword.
fn keyword_atom(name: &str) -> String {
    name.chars()
        .filter(|ch| {
            ch.is_ascii()
                && !ch.is_ascii_control()
                && !matches!(ch, '(' | ')' | '{' | ' ' | '%' | '*' | '"' | '\\' | ']')
        })
        .collect()
}
