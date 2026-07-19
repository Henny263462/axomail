//! Connection configuration for the SMTP adapter.

use std::time::Duration;

use axomail_core::{ConnectionCredential, ConnectionSecurity, ServerEndpoint};

/// Default deadline applied to every network operation when the caller does not
/// override [`SmtpConfig::timeout`].
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Everything the adapter needs to open one authenticated SMTP session.
///
/// The password is held in plaintext only for the lifetime of a send: the
/// engine resolves it from the OS keychain (`axomail-security`) immediately
/// before calling [`SmtpProvider::send`](crate::SmtpProvider) and drops it
/// afterwards. It is never logged, serialized, or placed in a
/// [`axomail_core::MailError`].
///
/// There is deliberately **no** option to accept invalid certificates. Trust is
/// the [`webpki-roots`](webpki_roots) bundle plus [`Self::extra_root_certs_der`];
/// nothing here can weaken certificate validation (see the crate-level docs).
#[derive(Clone)]
pub struct SmtpConfig {
    /// Submission host and port.
    pub endpoint: ServerEndpoint,
    /// Implicit TLS or `STARTTLS`; plaintext is not representable.
    pub security: ConnectionSecurity,
    /// SASL login name.
    pub username: String,
    /// SASL authentication material: a password (AUTH PLAIN/LOGIN) or an
    /// OAuth2 access token (AUTH XOAUTH2). Not secret-safe to store; see the
    /// type docs.
    pub credential: ConnectionCredential,
    /// Per-operation network deadline (connect, each command, DATA). Map to
    /// [`axomail_core::MailError::Timeout`] on expiry.
    pub timeout: Duration,
    /// Additional trust anchors, DER-encoded.
    ///
    /// Intended solely for the in-process TLS test server, whose self-signed
    /// certificate is not in any public root program. Production callers leave
    /// this empty. These roots are **added to** the public bundle, never
    /// replace it, and cannot disable validation.
    pub extra_root_certs_der: Vec<Vec<u8>>,
}

impl SmtpConfig {
    /// Builds a password configuration with the [`DEFAULT_TIMEOUT`] and no
    /// extra roots.
    #[must_use]
    pub fn new(
        endpoint: ServerEndpoint,
        security: ConnectionSecurity,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self::with_credential(
            endpoint,
            security,
            username,
            ConnectionCredential::password(password.into()),
        )
    }

    /// Builds an OAuth2 (`AUTH XOAUTH2`) configuration from an access token.
    #[must_use]
    pub fn new_xoauth2(
        endpoint: ServerEndpoint,
        security: ConnectionSecurity,
        username: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Self {
        Self::with_credential(
            endpoint,
            security,
            username,
            ConnectionCredential::xoauth2(access_token.into()),
        )
    }

    /// Builds a configuration from an explicit credential.
    #[must_use]
    pub fn with_credential(
        endpoint: ServerEndpoint,
        security: ConnectionSecurity,
        username: impl Into<String>,
        credential: ConnectionCredential,
    ) -> Self {
        Self {
            endpoint,
            security,
            username: username.into(),
            credential,
            timeout: DEFAULT_TIMEOUT,
            extra_root_certs_der: Vec::new(),
        }
    }
}

impl std::fmt::Debug for SmtpConfig {
    /// Redacts the credential and never prints login material.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("endpoint", &self.endpoint)
            .field("security", &self.security)
            .field("username", &"<redacted>")
            .field("credential", &self.credential)
            .field("timeout", &self.timeout)
            .field("extra_root_certs_der", &self.extra_root_certs_der.len())
            .finish()
    }
}
