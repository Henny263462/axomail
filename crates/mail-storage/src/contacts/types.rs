//! Value types for the contact repository: identifiers, the address-book
//! records, the create/update payloads, and the autocomplete/group projections.

use axomail_core::EmailAddress;
use serde::{Deserialize, Serialize};

/// Identifier of a persisted [`Contact`] row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContactId(i64);

impl ContactId {
    /// Wraps a raw database key.
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    /// Returns the raw database key.
    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
    }
}

impl std::fmt::Display for ContactId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifier of a persisted [`ContactGroup`] row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContactGroupId(i64);

impl ContactGroupId {
    /// Wraps a raw database key.
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    /// Returns the raw database key.
    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
    }
}

impl std::fmt::Display for ContactGroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One address belonging to a contact, before an identifier is assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewContactEmail {
    /// The validated address.
    pub email: EmailAddress,
    /// Presentation label ("home", "work", ...); empty when unlabeled.
    pub label: String,
    /// Whether this is the contact's primary address. The repository enforces
    /// exactly one primary per contact, promoting the first address if the
    /// caller marks none.
    pub is_primary: bool,
}

impl NewContactEmail {
    /// Builds an unlabeled, non-primary address (the auto-collect shape).
    #[must_use]
    pub fn plain(email: EmailAddress) -> Self {
        Self {
            email,
            label: String::new(),
            is_primary: false,
        }
    }
}

/// One persisted contact address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactEmail {
    /// The validated address.
    pub email: EmailAddress,
    /// Presentation label; empty when unlabeled.
    pub label: String,
    /// Whether this is the contact's primary address.
    pub is_primary: bool,
}

/// A contact to insert, before an identifier has been assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewContact {
    /// Human-readable name. Blanks are replaced by a name derived from the
    /// primary address on write, so a contact is never nameless.
    pub display_name: String,
    /// The contact's addresses. May be empty for a name-only contact.
    pub emails: Vec<NewContactEmail>,
    /// Free-form note; empty when unset.
    pub note: String,
}

/// The editable content of an existing contact (a full replacement of its
/// name, note, and address set).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactUpdate {
    /// New human-readable name.
    pub display_name: String,
    /// New address set (fully replaces the old one).
    pub emails: Vec<NewContactEmail>,
    /// New note.
    pub note: String,
}

/// A persisted contact with its addresses, primary first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    /// Row identifier.
    pub id: ContactId,
    /// Human-readable name.
    pub display_name: String,
    /// Denormalized primary address, or `None` for a name-only contact.
    pub primary_email: Option<EmailAddress>,
    /// Free-form note; empty when unset.
    pub note: String,
    /// Every address, primary first then by insertion order.
    pub emails: Vec<ContactEmail>,
    /// Creation time (Unix seconds).
    pub created_at_unix: i64,
    /// Last-updated time (Unix seconds).
    pub updated_at_unix: i64,
}

/// One autocomplete match: a contact address (or a name-matched contact) with
/// enough to render a suggestion and insert the address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactMatch {
    /// The matched contact.
    pub contact_id: ContactId,
    /// The contact's display name.
    pub display_name: String,
    /// The specific address this suggestion inserts.
    pub email: EmailAddress,
}

/// A persisted contact group (mailing list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactGroup {
    /// Row identifier.
    pub id: ContactGroupId,
    /// Group / mailing-list name.
    pub name: String,
    /// Number of member contacts.
    pub member_count: i64,
}
