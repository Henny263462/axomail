//! Data-transfer objects exchanged with the Vue frontend.
//!
//! Every field name matches the shared command contract; serialization uses
//! `camelCase`. DTOs carry only what list and detail views render — never
//! bodies, raw MIME, passwords, or credential references.
//!
//! # ID precision invariant
//!
//! Rust row identifiers are `i64` while JavaScript numbers are exact only up
//! to `2^53 - 1`. Every identifier that crosses the IPC boundary is checked
//! by [`checked_id`]; an out-of-range id yields a [`MailError::Configuration`]
//! instead of silently losing precision in the frontend (see the matching
//! note in `apps/desktop/src/types/mail.ts`).

use axomail_core::{
    Account, ConnectionSecurity, IncomingProtocol, MailError, Mailbox, MailboxRole, MessageEnvelope,
};
use axomail_storage::MessagePage;
use serde::{Deserialize, Serialize};

/// Largest integer a JavaScript `number` can represent exactly: `2^53 - 1`.
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Validates that a database identifier survives the crossing to JavaScript.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if `value` is negative or exceeds
/// [`MAX_SAFE_INTEGER`], the two cases in which a JavaScript `number` could
/// not represent it exactly.
pub fn checked_id(value: i64) -> Result<i64, MailError> {
    if (0..=MAX_SAFE_INTEGER).contains(&value) {
        Ok(value)
    } else {
        Err(MailError::Configuration {
            message: "identifier exceeds the safe integer range".to_owned(),
        })
    }
}

/// A mailbox as shown in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxSummaryDto {
    /// Local identifier.
    pub id: i64,
    /// Decoded folder name.
    pub name: String,
    /// Well-known role (`inbox`, `drafts`, ...).
    pub role: MailboxRole,
    /// Number of unread messages in the mailbox.
    pub unread_count: i64,
}

impl MailboxSummaryDto {
    /// Builds a mailbox summary, validating its id and attaching a
    /// separately computed unread count.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the mailbox id is out of the
    /// safe integer range.
    pub fn from_domain(mailbox: &Mailbox, unread_count: i64) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(mailbox.id.value())?,
            name: mailbox.name.as_str().to_owned(),
            role: mailbox.role,
            unread_count,
        })
    }
}

/// An account plus its mailbox tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummaryDto {
    /// Local identifier.
    pub id: i64,
    /// Name shown in the account list.
    pub display_name: String,
    /// Primary address.
    pub email: String,
    /// The account's mailboxes.
    pub mailboxes: Vec<MailboxSummaryDto>,
}

impl AccountSummaryDto {
    /// Builds an account summary from a domain account and its already-built
    /// mailbox summaries.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the account id is out of the
    /// safe integer range.
    pub fn new(account: &Account, mailboxes: Vec<MailboxSummaryDto>) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(account.id.value())?,
            display_name: account.display_name.clone(),
            email: account.email.as_str().to_owned(),
            mailboxes,
        })
    }
}

/// A single row in a message list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageListItemDto {
    /// Local identifier.
    pub id: i64,
    /// Owning mailbox.
    pub mailbox_id: i64,
    /// Decoded subject, if any.
    pub subject: Option<String>,
    /// Display name of the first sender, or an empty string.
    pub from_name: String,
    /// Address of the first sender, or an empty string.
    pub from_address: String,
    /// Date header in Unix seconds (UTC); `None` when it was absent.
    pub date_unix: Option<i64>,
    /// Whether the message has been read.
    pub seen: bool,
    /// Whether the message is flagged/starred.
    pub flagged: bool,
    /// Sanitized plain-text preview, no markup.
    pub preview: Option<String>,
}

impl MessageListItemDto {
    /// Builds a list item from a stored envelope, validating its ids.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the message or mailbox id is
    /// out of the safe integer range.
    pub fn from_envelope(envelope: &MessageEnvelope) -> Result<Self, MailError> {
        let sender = envelope.from.first();
        Ok(Self {
            id: checked_id(envelope.id.value())?,
            mailbox_id: checked_id(envelope.mailbox.value())?,
            subject: envelope.subject.clone(),
            from_name: sender
                .and_then(|address| address.display_name.clone())
                .unwrap_or_default(),
            from_address: sender
                .map(|address| address.email.as_str().to_owned())
                .unwrap_or_default(),
            date_unix: envelope.date.map(axomail_core::UtcTimestamp::unix_seconds),
            seen: envelope.flags.seen,
            flagged: envelope.flags.flagged,
            preview: envelope.preview.clone(),
        })
    }
}

/// One page of a message listing plus an opaque cursor for the next page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageListPageDto {
    /// Message rows, newest first.
    pub items: Vec<MessageListItemDto>,
    /// Opaque cursor token for the next page, or `None` on the last page.
    pub next_cursor: Option<String>,
}

impl MessageListPageDto {
    /// Maps a storage page to its DTO form, validating every id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if any id is out of the safe
    /// integer range.
    pub fn from_page(page: &MessagePage) -> Result<Self, MailError> {
        let items = page
            .items
            .iter()
            .map(MessageListItemDto::from_envelope)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            items,
            next_cursor: page.next.map(|cursor| cursor.token()),
        })
    }
}

/// One server's connection input from the account-setup form.
///
/// Holds a plaintext password only transiently while the command stores it
/// in the keychain; it is never persisted, logged, or echoed back.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInput {
    /// Server host name.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Transport security.
    pub security: ConnectionSecurity,
    /// Login name (not secret).
    pub username: String,
    /// Password, moved straight into the keychain.
    pub password: String,
}

/// Incoming-server input, extending [`ServerInput`] with a protocol.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingInput {
    /// Receiving protocol.
    pub protocol: IncomingProtocol,
    /// Server host name.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Transport security.
    pub security: ConnectionSecurity,
    /// Login name (not secret).
    pub username: String,
    /// Password, moved straight into the keychain.
    pub password: String,
}

/// Full input for creating an account.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAccountInput {
    /// Name shown in the account list.
    pub display_name: String,
    /// Primary address.
    pub email: String,
    /// Receiving configuration.
    pub incoming: IncomingInput,
    /// Sending configuration.
    pub outgoing: ServerInput,
}
