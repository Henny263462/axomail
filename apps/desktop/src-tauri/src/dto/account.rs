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

/// The receiving-server view of an [`AccountDetailDto`].
///
/// Carries the editable transport fields plus the immutable protocol and
/// login name, so the account-management form can show them; it never
/// carries the password or its credential reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingServerDetailDto {
    /// Receiving protocol (immutable; shown, never edited).
    pub protocol: IncomingProtocol,
    /// Server host name.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Transport security.
    pub security: ConnectionSecurity,
    /// Login name (immutable through the edit path; not secret).
    pub username: String,
}

/// The sending-server view of an [`AccountDetailDto`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingServerDetailDto {
    /// Server host name.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Transport security.
    pub security: ConnectionSecurity,
    /// Login name (immutable through the edit path; not secret).
    pub username: String,
}

/// The full, editable configuration of one account, without any secrets.
///
/// This is the read model behind the account-management section: it exposes
/// exactly the fields the edit form renders (display name and both servers'
/// host/port/security), plus the immutable protocol, login names, and email
/// for context. Passwords and credential references never appear here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDetailDto {
    /// Local identifier.
    pub id: i64,
    /// Name shown in the account list.
    pub display_name: String,
    /// Primary address (immutable through the edit path).
    pub email: String,
    /// Receiving configuration.
    pub incoming: IncomingServerDetailDto,
    /// Sending configuration.
    pub outgoing: OutgoingServerDetailDto,
}

impl AccountDetailDto {
    /// Builds a detail view from a domain account.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the account id is out of the
    /// safe integer range.
    pub fn from_domain(account: &Account) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(account.id.value())?,
            display_name: account.display_name.clone(),
            email: account.email.as_str().to_owned(),
            incoming: IncomingServerDetailDto {
                protocol: account.incoming.protocol,
                host: account.incoming.endpoint.host().to_owned(),
                port: account.incoming.endpoint.port().get(),
                security: account.incoming.security,
                username: account.incoming.username.clone(),
            },
            outgoing: OutgoingServerDetailDto {
                host: account.outgoing.endpoint.host().to_owned(),
                port: account.outgoing.endpoint.port().get(),
                security: account.outgoing.security,
                username: account.outgoing.username.clone(),
            },
        })
    }
}

/// One server's editable transport fields from the account-management form.
///
/// Unlike [`ServerInput`], this carries no password and no protocol: an edit
/// can change only the host, port, and transport security. The protocol and
/// credentials of an existing account are immutable through this path.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerEndpointInput {
    /// Server host name.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Transport security.
    pub security: ConnectionSecurity,
}

/// Input for the `update_account` command.
///
/// Names exactly the mutable fields; the absence of protocol, username,
/// email, and password fields is intentional — those cannot be changed by
/// an edit, so the wire shape cannot even express such a change.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAccountInput {
    /// Account to update.
    pub account_id: i64,
    /// New display name.
    pub display_name: String,
    /// New receiving transport configuration.
    pub incoming: ServerEndpointInput,
    /// New sending transport configuration.
    pub outgoing: ServerEndpointInput,
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
