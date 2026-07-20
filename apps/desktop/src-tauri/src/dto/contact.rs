//! Contact / address-book DTOs: the address-book records, the create/update
//! input, group records, the recipient-autocomplete suggestion, and the vCard
//! import outcome.

use axomail_core::MailError;
use axomail_storage::{Contact, ContactEmail, ContactGroup};
use serde::{Deserialize, Serialize};

use super::checked_id;

/// One address on a contact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactEmailDto {
    /// The address.
    pub email: String,
    /// Presentation label ("home", "work", ...); empty when none.
    pub label: String,
    /// Whether this is the contact's primary address.
    pub is_primary: bool,
}

impl ContactEmailDto {
    fn from_email(email: &ContactEmail) -> Self {
        Self {
            email: email.email.as_str().to_owned(),
            label: email.label.clone(),
            is_primary: email.is_primary,
        }
    }
}

/// A full contact record for the address-book list and detail views.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactDto {
    /// Local identifier.
    pub id: i64,
    /// Human-readable name.
    pub display_name: String,
    /// Denormalized primary address, or `None` for a name-only contact.
    pub primary_email: Option<String>,
    /// Free-form note; empty when unset.
    pub note: String,
    /// Every address, primary first.
    pub emails: Vec<ContactEmailDto>,
}

impl ContactDto {
    /// Builds a contact DTO from a stored contact, validating its id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the contact id is out of the
    /// safe integer range.
    pub fn from_contact(contact: &Contact) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(contact.id.value())?,
            display_name: contact.display_name.clone(),
            primary_email: contact
                .primary_email
                .as_ref()
                .map(|e| e.as_str().to_owned()),
            note: contact.note.clone(),
            emails: contact
                .emails
                .iter()
                .map(ContactEmailDto::from_email)
                .collect(),
        })
    }
}

/// A contact with the ids of the groups it belongs to (the editor payload).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactDetailDto {
    /// The contact record.
    #[serde(flatten)]
    pub contact: ContactDto,
    /// Groups the contact is a member of.
    pub group_ids: Vec<i64>,
}

/// One address in a create/update request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactEmailInput {
    /// The address; validated by the command before storage.
    pub email: String,
    /// Presentation label; may be empty.
    #[serde(default)]
    pub label: String,
    /// Whether this address is the primary one.
    #[serde(default)]
    pub is_primary: bool,
}

/// The create/update payload for a contact.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactInput {
    /// Human-readable name (a blank name is derived from the primary address).
    #[serde(default)]
    pub display_name: String,
    /// The contact's addresses (may be empty).
    #[serde(default)]
    pub emails: Vec<ContactEmailInput>,
    /// Free-form note.
    #[serde(default)]
    pub note: String,
}

/// A contact group (mailing list) with its member count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactGroupDto {
    /// Local identifier.
    pub id: i64,
    /// Group name.
    pub name: String,
    /// Number of member contacts.
    pub member_count: i64,
}

impl ContactGroupDto {
    /// Builds a group DTO from a stored group, validating its id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the group id is out of the safe
    /// integer range.
    pub fn from_group(group: &ContactGroup) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(group.id.value())?,
            name: group.name.clone(),
            member_count: group.member_count,
        })
    }
}

/// One recipient-autocomplete suggestion: a contact address or a whole group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipientSuggestionDto {
    /// `"contact"` or `"group"`.
    pub kind: &'static str,
    /// Primary line shown in the dropdown (contact name or group name).
    pub label: String,
    /// Secondary line (the address, or an `"N members"` count).
    pub detail: String,
    /// The text inserted into the recipient field: a bare address for a
    /// contact, or the comma-joined member addresses for a group.
    pub value: String,
}

/// Outcome of a vCard import: how many contacts were added versus skipped as
/// already present (matched by an existing address).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VCardImportDto {
    /// New contacts created.
    pub imported: u32,
    /// Cards skipped because one of their addresses already existed.
    pub skipped: u32,
}
