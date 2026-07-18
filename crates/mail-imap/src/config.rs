//! Connection configuration for the IMAP adapter.

use std::time::Duration;

use axomail_core::{ConnectionSecurity, ServerEndpoint};

/// Default deadline applied to every individual network operation.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Everything the adapter needs to open and authenticate an IMAP session.
///
/// The password is held in plain text only for the lifetime of the
/// connection attempt; it originates from the OS keychain (resolved by
/// `axomail-security`) and never appears in logs or errors.
#[derive(Clone)]
pub struct ImapConfig {
    /// Host and port of the IMAP server.
    pub endpoint: ServerEndpoint,
    /// Whether the transport is implicit TLS or a `STARTTLS` upgrade.
    pub security: ConnectionSecurity,
    /// Login name presented to `LOGIN`.
    pub username: String,
    /// Password presented to `LOGIN`.
    pub password: String,
    /// Deadline applied to each network operation
    /// ([`DEFAULT_TIMEOUT`] is a sensible value).
    pub timeout: Duration,
    /// Additional trust anchors in DER form.
    ///
    /// This exists **only** to trust the self-signed certificate of a
    /// local test server. It is additive: the Mozilla root set is always
    /// active as well, and there is deliberately no switch to disable
    /// certificate verification.
    pub extra_root_certs_der: Vec<Vec<u8>>,
}

impl ImapConfig {
    /// Builds a configuration with the default per-operation timeout and
    /// no extra trust anchors.
    #[must_use]
    pub fn new(
        endpoint: ServerEndpoint,
        security: ConnectionSecurity,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            endpoint,
            security,
            username: username.into(),
            password: password.into(),
            timeout: DEFAULT_TIMEOUT,
            extra_root_certs_der: Vec::new(),
        }
    }
}

impl std::fmt::Debug for ImapConfig {
    /// Debug output deliberately omits the password.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImapConfig")
            .field("endpoint", &self.endpoint)
            .field("security", &self.security)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .field("timeout", &self.timeout)
            .field("extra_root_certs_der", &self.extra_root_certs_der.len())
            .finish()
    }
}
