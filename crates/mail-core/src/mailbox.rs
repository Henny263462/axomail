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
///
/// `name` is the unique display label (historically the full decoded path);
/// `remote_name` is the authoritative full server path the discovery reconcile
/// upserts by. For a flat/local folder the two coincide and `parent`,
/// `delimiter` are `None`. The tree derives a leaf label from `remote_name`
/// split on `delimiter` (see [`Mailbox::leaf_name`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mailbox {
    /// Local identifier.
    pub id: MailboxId,
    /// Owning account.
    pub account: AccountId,
    /// Decoded, unique display name.
    pub name: MailboxName,
    /// Detected or server-declared role.
    pub role: MailboxRole,
    /// Containing folder, or `None` for a top-level folder.
    pub parent: Option<MailboxId>,
    /// Full server path this folder is addressed by (e.g. `INBOX/Work`).
    pub remote_name: MailboxName,
    /// Server hierarchy delimiter for this folder, or `None` when flat.
    pub delimiter: Option<String>,
    /// Whether the folder is subscribed (shown by default).
    pub subscribed: bool,
    /// Whether the folder vanished from the server but still holds local
    /// messages, so it is kept and flagged rather than dropped.
    pub orphaned: bool,
}

impl Mailbox {
    /// The last path segment of [`Mailbox::remote_name`], split on the folder's
    /// [`Mailbox::delimiter`]. Falls back to the whole display `name` when no
    /// delimiter is known (flat folders). Never returns an empty string.
    #[must_use]
    pub fn leaf_name(&self) -> &str {
        leaf_of(self.remote_name.as_str(), self.delimiter.as_deref())
            .filter(|leaf| !leaf.is_empty())
            .unwrap_or_else(|| self.name.as_str())
    }

    /// The parent path of [`Mailbox::remote_name`] (everything before the last
    /// delimiter segment), or `None` when the folder is top-level or flat.
    #[must_use]
    pub fn parent_remote_name(&self) -> Option<&str> {
        let delimiter = self.delimiter.as_deref()?;
        if delimiter.is_empty() {
            return None;
        }
        let path = self.remote_name.as_str();
        path.rfind(delimiter).map(|idx| &path[..idx])
    }
}

/// Splits `path` on `delimiter`, returning the final segment. `None` when the
/// delimiter is absent/empty so the caller can fall back to the display name.
fn leaf_of<'a>(path: &'a str, delimiter: Option<&str>) -> Option<&'a str> {
    let delimiter = delimiter?;
    if delimiter.is_empty() {
        return None;
    }
    Some(path.rsplit(delimiter).next().unwrap_or(path))
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

    fn mailbox(name: &str, remote: &str, delimiter: Option<&str>) -> Mailbox {
        Mailbox {
            id: MailboxId::new(1),
            account: AccountId::new(1),
            name: MailboxName::new(name),
            role: MailboxRole::Custom,
            parent: None,
            remote_name: MailboxName::new(remote),
            delimiter: delimiter.map(str::to_owned),
            subscribed: true,
            orphaned: false,
        }
    }

    #[test]
    fn leaf_name_splits_on_delimiter() {
        assert_eq!(
            mailbox("INBOX/Work/2026", "INBOX/Work/2026", Some("/")).leaf_name(),
            "2026"
        );
        assert_eq!(mailbox("Sent", "Sent", Some("/")).leaf_name(), "Sent");
        // No delimiter: fall back to the whole display name.
        assert_eq!(mailbox("Inbox", "INBOX", None).leaf_name(), "Inbox");
        // A trailing-delimiter path never yields an empty leaf.
        assert_eq!(mailbox("Odd/", "Odd/", Some("/")).leaf_name(), "Odd/");
    }

    #[test]
    fn parent_remote_name_strips_last_segment() {
        assert_eq!(
            mailbox("INBOX/Work/2026", "INBOX/Work/2026", Some("/")).parent_remote_name(),
            Some("INBOX/Work")
        );
        assert_eq!(
            mailbox("INBOX", "INBOX", Some("/")).parent_remote_name(),
            None
        );
        assert_eq!(mailbox("Inbox", "INBOX", None).parent_remote_name(), None);
    }
}
