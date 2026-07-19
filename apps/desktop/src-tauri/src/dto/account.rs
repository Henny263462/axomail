//! Account, mailbox, and account-setup DTOs.

use axomail_core::{
    Account, AccountAuth, AuthMechanism, ConnectionSecurity, HttpsUrl, IncomingProtocol, MailError,
    Mailbox, MailboxRole, OAuthClientId, OAuthConfig, OAuthProvider, OAuthScopes,
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
    /// How the account authenticates (`password` or `oauth2`), so the UI can
    /// offer "Sign in with browser" for OAuth accounts. Non-secret.
    pub auth_mechanism: AuthMechanism,
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
            auth_mechanism: account.auth.mechanism(),
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
    /// Authentication choice. Absent input defaults to password so existing
    /// callers keep working.
    #[serde(default)]
    pub auth: AuthInput,
    /// Receiving configuration.
    pub incoming: IncomingInput,
    /// Sending configuration.
    pub outgoing: ServerInput,
}

/// The account's chosen authentication mechanism and, for OAuth2, its
/// non-secret configuration. No tokens or passwords are carried here.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthInput {
    /// `password` or `oauth2`.
    pub mechanism: AuthMechanism,
    /// OAuth2 configuration, required when `mechanism` is `oauth2`.
    #[serde(default)]
    pub oauth: Option<OAuthInput>,
}

impl Default for AuthInput {
    fn default() -> Self {
        Self {
            mechanism: AuthMechanism::Password,
            oauth: None,
        }
    }
}

impl AuthInput {
    /// Resolves the input into a validated [`AccountAuth`].
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] when an OAuth2 mechanism is chosen
    /// without configuration, or when any OAuth field is invalid.
    pub fn into_account_auth(self) -> Result<AccountAuth, MailError> {
        match self.mechanism {
            AuthMechanism::Password => Ok(AccountAuth::Password),
            AuthMechanism::OAuth2 => {
                let oauth = self.oauth.ok_or_else(|| MailError::Configuration {
                    message: "OAuth2 was selected but no provider configuration was supplied"
                        .to_owned(),
                })?;
                Ok(AccountAuth::OAuth2(oauth.into_config()?))
            }
        }
    }
}

/// OAuth2 configuration from the setup form: a provider preset (or custom),
/// the user-supplied client id, and optional endpoint/scope overrides.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthInput {
    /// Preset provider, or `custom`.
    pub provider: OAuthProvider,
    /// User-supplied public client id (axomail ships none).
    pub client_id: String,
    /// Authorization endpoint; required for `custom`, overrides a preset.
    #[serde(default)]
    pub auth_url: Option<String>,
    /// Token endpoint; required for `custom`, overrides a preset.
    #[serde(default)]
    pub token_url: Option<String>,
    /// Scopes; required for `custom`, override a preset.
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
}

impl OAuthInput {
    /// Builds a validated [`OAuthConfig`], filling any omitted endpoint or
    /// scope from the provider preset.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] for an invalid client id or
    /// endpoint/scope, or for a `custom` provider missing a required field.
    pub fn into_config(self) -> Result<OAuthConfig, MailError> {
        let client_id = OAuthClientId::parse(self.client_id)?;
        let preset = self.provider.preset();
        let auth_url = resolve_url(
            self.auth_url,
            preset.as_ref().map(|p| &p.auth_url),
            "auth_url",
        )?;
        let token_url = resolve_url(
            self.token_url,
            preset.as_ref().map(|p| &p.token_url),
            "token_url",
        )?;
        let scopes = match self.scopes {
            Some(scopes) => OAuthScopes::new(scopes)?,
            None => preset
                .map(|p| p.scopes)
                .ok_or_else(|| MailError::Configuration {
                    message: "a custom OAuth provider requires scopes".to_owned(),
                })?,
        };
        Ok(OAuthConfig {
            client_id,
            auth_url,
            token_url,
            scopes,
        })
    }
}

/// Picks a supplied URL override, else the preset default, validating either.
fn resolve_url(
    supplied: Option<String>,
    preset: Option<&HttpsUrl>,
    field: &str,
) -> Result<HttpsUrl, MailError> {
    match supplied {
        Some(value) => HttpsUrl::parse(value),
        None => preset.cloned().ok_or_else(|| MailError::Configuration {
            message: format!("a custom OAuth provider requires {field}"),
        }),
    }
}
