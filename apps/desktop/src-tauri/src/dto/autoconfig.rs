//! Auto-configuration DTOs.
//!
//! These mirror the internal [`AutoconfigOutcome`](crate::autoconfig) into the
//! camelCase wire shape the setup dialog consumes. No secret is involved: a
//! result is only a *suggestion* of host/port/security/username, which the
//! user reviews, tests, and may edit before an account is created.

use axomail_core::{ConnectionSecurity, IncomingProtocol};
use serde::Serialize;

use crate::autoconfig::AutoconfigOutcome;

/// Suggested incoming (IMAP/POP3) server settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoconfigIncomingDto {
    /// Receiving protocol.
    pub protocol: IncomingProtocol,
    /// Server host name.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Transport security.
    pub security: ConnectionSecurity,
    /// Login name (placeholders already resolved).
    pub username: String,
}

/// Suggested outgoing (SMTP) server settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoconfigOutgoingDto {
    /// Server host name.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Transport security.
    pub security: ConnectionSecurity,
    /// Login name (placeholders already resolved).
    pub username: String,
}

/// The outcome of an auto-configuration lookup.
///
/// A single struct (rather than a bare `Option`) is returned so the frontend
/// distinguishes "not found" (`found: false`) from the browser-preview /
/// IPC-absent case (`null`), which need different guidance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoconfigResultDto {
    /// Whether a configuration was discovered.
    pub found: bool,
    /// Which source produced it (`autoconfig` / `ispdb` / `guess`), or `null`.
    pub source: Option<String>,
    /// Whether the provider prefers OAuth2 authentication.
    pub oauth: bool,
    /// Provider display name, when advertised.
    pub display_name: Option<String>,
    /// Suggested incoming server, or `null` when not found.
    pub incoming: Option<AutoconfigIncomingDto>,
    /// Suggested outgoing server, or `null` when not found.
    pub outgoing: Option<AutoconfigOutgoingDto>,
}

impl AutoconfigResultDto {
    /// Builds a found result from a discovery outcome.
    #[must_use]
    pub fn found(outcome: &AutoconfigOutcome) -> Self {
        let config = &outcome.config;
        Self {
            found: true,
            source: Some(outcome.source.as_str().to_owned()),
            oauth: config.incoming.oauth,
            display_name: config.display_name.clone(),
            incoming: Some(AutoconfigIncomingDto {
                protocol: config.incoming.protocol,
                host: config.incoming.host.clone(),
                port: config.incoming.port,
                security: config.incoming.security,
                username: config.incoming.username.clone(),
            }),
            outgoing: Some(AutoconfigOutgoingDto {
                host: config.outgoing.host.clone(),
                port: config.outgoing.port,
                security: config.outgoing.security,
                username: config.outgoing.username.clone(),
            }),
        }
    }

    /// Builds the "nothing found" result.
    #[must_use]
    pub fn not_found() -> Self {
        Self {
            found: false,
            source: None,
            oauth: false,
            display_name: None,
            incoming: None,
            outgoing: None,
        }
    }
}
