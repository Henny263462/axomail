//! Connection configuration for the POP3 adapter.

use std::time::Duration;

use axomail_core::{ConnectionSecurity, ServerEndpoint};

/// Default deadline applied to every individual network operation.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Everything the adapter needs to open and authenticate a POP3 session.
///
/// The password is held in plain text only for the lifetime of the
/// connection; it originates from the OS keychain (resolved by
/// `axomail-security`) and never appears in logs or errors.
#[derive(Clone)]
pub struct Pop3Config {
    /// Host and port of the POP3 server.
    pub endpoint: ServerEndpoint,
    /// Whether the transport is implicit TLS or an `STLS` upgrade.
    pub security: ConnectionSecurity,
    /// Login name presented to `USER`.
    pub username: String,
    /// Password presented to `PASS`.
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
    /// Whether the adapter is permitted to delete messages on the server
    /// after they have been downloaded.
    ///
    /// This is the **first** of a deliberate double guard against POP3's
    /// destructive, irreversible `DELE`. It defaults to `false`, so the
    /// adapter never removes server-side mail unless the account is
    /// explicitly configured for download-and-delete.
    ///
    /// The second guard lives in the command/composition layer: even with
    /// this flag set, that layer must independently confirm the user's
    /// intent for each deletion before calling
    /// [`delete_message`](axomail_core::IncomingMailProvider::delete_message).
    /// The adapter never deletes silently or as a side effect of syncing;
    /// this flag only lifts the hard block, it does not request deletion.
    pub delete_after_download: bool,
}

impl Pop3Config {
    /// Builds a configuration with the default per-operation timeout, no
    /// extra trust anchors, and server-side deletion disabled.
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
            delete_after_download: false,
        }
    }
}

impl std::fmt::Debug for Pop3Config {
    /// Debug output deliberately omits the password.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pop3Config")
            .field("endpoint", &self.endpoint)
            .field("security", &self.security)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .field("timeout", &self.timeout)
            .field("extra_root_certs_der", &self.extra_root_certs_der.len())
            .field("delete_after_download", &self.delete_after_download)
            .finish()
    }
}
