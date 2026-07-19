//! Account and server configuration models.
//!
//! These types intentionally cannot hold secrets: credentials are
//! referenced through [`CredentialRef`] and resolved exclusively by
//! `mail-security` at connection time.

use std::num::NonZeroU16;

use serde::{Deserialize, Serialize};

use crate::address::EmailAddress;
use crate::error::MailError;
use crate::id::{AccountId, CredentialRef};
use crate::oauth::AccountAuth;

/// Protocol used to receive mail for an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IncomingProtocol {
    /// Two-way synchronization; the server owns folders and flags.
    Imap,
    /// Download-only source; local storage owns folders and flags.
    Pop3,
}

/// Transport security for a server connection.
///
/// Plaintext connections are not representable by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionSecurity {
    /// Implicit TLS from the first byte.
    Tls,
    /// Plaintext connect upgraded via `STARTTLS` before any credentials
    /// or mail data are exchanged.
    StartTls,
}

/// Network endpoint of a mail server.
///
/// Fields are private so every instance — including deserialized ones —
/// passes host validation ([`ServerEndpoint::new`] is the only door).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawServerEndpoint")]
pub struct ServerEndpoint {
    host: String,
    port: NonZeroU16,
}

/// Unvalidated wire form of [`ServerEndpoint`], used only by serde.
#[derive(Deserialize)]
struct RawServerEndpoint {
    host: String,
    port: NonZeroU16,
}

impl TryFrom<RawServerEndpoint> for ServerEndpoint {
    type Error = MailError;

    fn try_from(raw: RawServerEndpoint) -> Result<Self, Self::Error> {
        Self::new(raw.host, raw.port)
    }
}

impl ServerEndpoint {
    /// Creates an endpoint after validation of the host name.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] for an empty host, a host
    /// containing whitespace or control characters, or a host containing
    /// URI delimiters (`/`, `@`, `?`, `#`) that could smuggle userinfo
    /// or paths into what must be a bare host name.
    pub fn new(host: impl Into<String>, port: NonZeroU16) -> Result<Self, MailError> {
        let host = host.into();
        if host.is_empty() {
            return Err(MailError::Configuration {
                message: "server host must not be empty".into(),
            });
        }
        if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(MailError::Configuration {
                message: "server host must not contain whitespace".into(),
            });
        }
        if host
            .chars()
            .any(|c| matches!(c, '/' | '@' | '?' | '#' | '\\'))
        {
            return Err(MailError::Configuration {
                message: "server host must be a bare host name, not a URI".into(),
            });
        }
        Ok(Self { host, port })
    }

    /// Returns the validated host name.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the TCP port.
    #[must_use]
    pub fn port(&self) -> NonZeroU16 {
        self.port
    }
}

impl IncomingProtocol {
    /// Conventional default port for the protocol/security combination.
    #[must_use]
    pub fn default_port(self, security: ConnectionSecurity) -> NonZeroU16 {
        let port = match (self, security) {
            (Self::Imap, ConnectionSecurity::Tls) => 993,
            (Self::Imap, ConnectionSecurity::StartTls) => 143,
            (Self::Pop3, ConnectionSecurity::Tls) => 995,
            (Self::Pop3, ConnectionSecurity::StartTls) => 110,
        };
        NonZeroU16::new(port).unwrap_or(NonZeroU16::MIN)
    }
}

/// Configuration for receiving mail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncomingServerConfig {
    /// Receiving protocol.
    pub protocol: IncomingProtocol,
    /// Server endpoint.
    pub endpoint: ServerEndpoint,
    /// Transport security; plaintext is not available.
    pub security: ConnectionSecurity,
    /// Login name (not secret).
    pub username: String,
    /// Keychain reference to the password; never the password itself.
    pub credential: CredentialRef,
}

/// Configuration for sending mail via SMTP submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingServerConfig {
    /// Server endpoint.
    pub endpoint: ServerEndpoint,
    /// Transport security; plaintext is not available.
    pub security: ConnectionSecurity,
    /// Login name (not secret).
    pub username: String,
    /// Keychain reference to the password; never the password itself.
    pub credential: CredentialRef,
}

impl OutgoingServerConfig {
    /// Conventional default SMTP submission port for a security mode.
    #[must_use]
    pub fn default_port(security: ConnectionSecurity) -> NonZeroU16 {
        let port = match security {
            ConnectionSecurity::Tls => 465,
            ConnectionSecurity::StartTls => 587,
        };
        NonZeroU16::new(port).unwrap_or(NonZeroU16::MIN)
    }
}

/// A configured mail account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// Local identifier.
    pub id: AccountId,
    /// Name shown in the account list.
    pub display_name: String,
    /// Primary address used as the default sender.
    pub email: EmailAddress,
    /// How the account authenticates (password or OAuth2). The secret itself
    /// is never held here — only the non-secret mechanism and OAuth endpoints.
    #[serde(default = "AccountAuth::default_password")]
    pub auth: AccountAuth,
    /// Receiving configuration.
    pub incoming: IncomingServerConfig,
    /// Sending configuration.
    pub outgoing: OutgoingServerConfig,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn default_ports_match_conventions() {
        use ConnectionSecurity::{StartTls, Tls};
        assert_eq!(IncomingProtocol::Imap.default_port(Tls).get(), 993);
        assert_eq!(IncomingProtocol::Imap.default_port(StartTls).get(), 143);
        assert_eq!(IncomingProtocol::Pop3.default_port(Tls).get(), 995);
        assert_eq!(IncomingProtocol::Pop3.default_port(StartTls).get(), 110);
        assert_eq!(OutgoingServerConfig::default_port(Tls).get(), 465);
        assert_eq!(OutgoingServerConfig::default_port(StartTls).get(), 587);
    }

    #[test]
    fn endpoint_rejects_invalid_hosts() {
        let port = NonZeroU16::new(993).unwrap();
        assert!(ServerEndpoint::new("", port).is_err());
        assert!(ServerEndpoint::new("has space.example.org", port).is_err());
        assert!(ServerEndpoint::new("user@evil.example.org", port).is_err());
        assert!(ServerEndpoint::new("host.example.org/path", port).is_err());
        assert!(ServerEndpoint::new("imap.example.org", port).is_ok());
    }

    #[test]
    fn endpoint_deserialization_cannot_bypass_validation() {
        let bad: Result<ServerEndpoint, _> =
            serde_json::from_str(r#"{"host": "user@evil.example.org", "port": 993}"#);
        assert!(bad.is_err());
        let ok: Result<ServerEndpoint, _> =
            serde_json::from_str(r#"{"host": "imap.example.org", "port": 993}"#);
        assert!(ok.is_ok());
    }

    #[test]
    fn zero_port_is_unrepresentable() {
        // Compile-time guarantee via NonZeroU16; this documents intent.
        assert!(NonZeroU16::new(0).is_none());
    }

    #[test]
    fn account_serializes_without_secret_fields() {
        let account = Account {
            id: AccountId::new(1),
            display_name: "Work".into(),
            email: EmailAddress::parse("k@example.org").unwrap(),
            auth: AccountAuth::Password,
            incoming: IncomingServerConfig {
                protocol: IncomingProtocol::Imap,
                endpoint: ServerEndpoint::new("imap.example.org", NonZeroU16::new(993).unwrap())
                    .unwrap(),
                security: ConnectionSecurity::Tls,
                username: "k@example.org".into(),
                credential: CredentialRef::new("axomail/account/1/incoming"),
            },
            outgoing: OutgoingServerConfig {
                endpoint: ServerEndpoint::new("smtp.example.org", NonZeroU16::new(465).unwrap())
                    .unwrap(),
                security: ConnectionSecurity::Tls,
                username: "k@example.org".into(),
                credential: CredentialRef::new("axomail/account/1/outgoing"),
            },
        };
        let json = serde_json::to_string(&account).unwrap();
        assert!(json.contains("axomail/account/1/incoming"));
        // The mechanism token "password" is expected; what must never appear
        // is secret material. Only non-secret references are serialized.
        assert_eq!(account.auth.mechanism(), crate::AuthMechanism::Password);
        assert!(json.contains("\"auth\":\"password\""));
        assert!(!json.contains("hunter2"));
    }
}
