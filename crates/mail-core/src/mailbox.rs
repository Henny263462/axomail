//! Mailbox (folder) models.

use serde::{Deserialize, Serialize};

use crate::id::{AccountId, MailboxId};

/// Decoded, human-readable mailbox name.
///
/// Adapters decode protocol encodings (e.g. IMAP modified UTF-7) before
/// constructing this type; the core only ever sees UTF-8.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MailboxName(String);

impl MailboxName {
    /// Wraps a decoded mailbox name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MailboxName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Well-known role of a mailbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MailboxRole {
    /// Primary incoming mailbox.
    Inbox,
    /// Unsent drafts.
    Drafts,
    /// Sent messages.
    Sent,
    /// Deleted messages awaiting expunge.
    Trash,
    /// Archived messages.
    Archive,
    /// Suspected spam.
    Junk,
    /// No recognized special role.
    Custom,
}

impl MailboxRole {
    /// Best-effort role detection from a mailbox name.
    ///
    /// This is only a fallback: adapters should prefer server-provided
    /// attributes (IMAP `SPECIAL-USE`, RFC 6154) when available.
    #[must_use]
    pub fn guess_from_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "inbox" => Self::Inbox,
            "drafts" | "draft" => Self::Drafts,
            "sent" | "sent items" | "sent messages" => Self::Sent,
            "trash" | "deleted" | "deleted items" | "deleted messages" => Self::Trash,
            "archive" | "archives" | "all mail" => Self::Archive,
            "junk" | "spam" | "junk e-mail" => Self::Junk,
            _ => Self::Custom,
        }
    }
}

/// A mailbox within an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mailbox {
    /// Local identifier.
    pub id: MailboxId,
    /// Owning account.
    pub account: AccountId,
    /// Decoded name.
    pub name: MailboxName,
    /// Detected or server-declared role.
    pub role: MailboxRole,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_common_names_case_insensitively() {
        assert_eq!(MailboxRole::guess_from_name("INBOX"), MailboxRole::Inbox);
        assert_eq!(MailboxRole::guess_from_name("Sent"), MailboxRole::Sent);
        assert_eq!(
            MailboxRole::guess_from_name("Deleted Items"),
            MailboxRole::Trash
        );
        assert_eq!(MailboxRole::guess_from_name("Spam"), MailboxRole::Junk);
        assert_eq!(
            MailboxRole::guess_from_name("Projects/2026"),
            MailboxRole::Custom
        );
    }
}
