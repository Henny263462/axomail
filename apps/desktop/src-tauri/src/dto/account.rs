//! Account, mailbox, and account-setup DTOs.

use axomail_core::{
    Account, ConnectionSecurity, IncomingProtocol, MailError, Mailbox, MailboxRole,
};
use serde::{Deserialize, Serialize};

use super::checked_id;

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

/// One server's connection input from the account-setup form.
///
/// Holds a plaintext password only transiently while the command stores it
/// in the keychain; it is never persisted, logged, or echoed back. The
/// manual [`std::fmt::Debug`] impls below redact the password so even a
/// future `{:?}`/`?input` trace cannot leak it.
#[derive(Clone, Deserialize)]
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

impl std::fmt::Debug for ServerInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerInput")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("security", &self.security)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// Incoming-server input, extending [`ServerInput`] with a protocol.
#[derive(Clone, Deserialize)]
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

impl std::fmt::Debug for IncomingInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IncomingInput")
            .field("protocol", &self.protocol)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("security", &self.security)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// Full input for creating an account (also used by `test_connection`).
/// The derived `Debug` is safe: both embedded inputs redact their
/// password fields.
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
