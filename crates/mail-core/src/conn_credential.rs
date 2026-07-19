//! Transient authentication material handed to a protocol adapter.
//!
//! Unlike the persisted [`Account`](crate::Account) model, which can never
//! hold a secret, an adapter needs the actual password or OAuth2 access token
//! for the lifetime of a single connection attempt. [`ConnectionCredential`]
//! carries exactly that and nothing more; its [`Debug`] implementation is
//! hand-written so a stray `{:?}` can never print the secret.
//!
//! The [`xoauth2_sasl`] helper builds the `XOAUTH2` SASL initial-client
//! response common to IMAP, SMTP, and POP3 (RFC 7628 / Google & Microsoft
//! XOAUTH2). Base64 encoding is left to the caller because each protocol layer
//! frames it differently (async-imap encodes the authenticator output itself,
//! the POP3 adapter emits `AUTH XOAUTH2 <base64>` on the wire, and `mail-send`
//! builds the string from username + token internally).

/// Authentication material for one connection attempt.
///
/// The variants mirror [`AuthMechanism`](crate::AuthMechanism): a stored
/// password, or an already-resolved OAuth2 access token (refreshed if it was
/// due before it reaches the adapter).
#[derive(Clone, PartialEq, Eq)]
pub enum ConnectionCredential {
    /// A plain password presented via `LOGIN` / `AUTH PLAIN` / `USER`+`PASS`.
    Password(String),
    /// A bearer access token presented via the `XOAUTH2` SASL mechanism.
    XOauth2 {
        /// The OAuth2 access token (never a refresh token).
        access_token: String,
    },
}

impl ConnectionCredential {
    /// Wraps a password.
    #[must_use]
    pub fn password(secret: impl Into<String>) -> Self {
        Self::Password(secret.into())
    }

    /// Wraps an OAuth2 access token.
    #[must_use]
    pub fn xoauth2(access_token: impl Into<String>) -> Self {
        Self::XOauth2 {
            access_token: access_token.into(),
        }
    }
}

impl std::fmt::Debug for ConnectionCredential {
    /// Never prints the secret; only which mechanism is in use.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password(_) => f.write_str("ConnectionCredential::Password(<redacted>)"),
            Self::XOauth2 { .. } => {
                f.write_str("ConnectionCredential::XOauth2 { access_token: <redacted> }")
            }
        }
    }
}

/// Builds the `XOAUTH2` SASL initial-client response for `username` and
/// `access_token`.
///
/// The exact byte string is
/// `user=<username>^Aauth=Bearer <access_token>^A^A`, where `^A` is the
/// `Ctrl-A` (`0x01`) separator. The result is **not** base64-encoded; callers
/// frame and encode it as their protocol requires.
#[must_use]
pub fn xoauth2_sasl(username: &str, access_token: &str) -> String {
    format!("user={username}\x01auth=Bearer {access_token}\x01\x01")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xoauth2_sasl_matches_rfc_layout() {
        let sasl = xoauth2_sasl("user@example.org", "vF9dft4qmT");
        assert_eq!(
            sasl,
            "user=user@example.org\x01auth=Bearer vF9dft4qmT\x01\x01"
        );
    }

    #[test]
    fn debug_redacts_password() {
        let credential = ConnectionCredential::password("hunter2");
        let rendered = format!("{credential:?}");
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("hunter2"));
    }

    #[test]
    fn debug_redacts_access_token() {
        let credential = ConnectionCredential::xoauth2("ya29.secret-token");
        let rendered = format!("{credential:?}");
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("ya29.secret-token"));
    }

    #[test]
    fn constructors_wrap_expected_variants() {
        assert!(matches!(
            ConnectionCredential::password("x"),
            ConnectionCredential::Password(_)
        ));
        assert!(matches!(
            ConnectionCredential::xoauth2("t"),
            ConnectionCredential::XOauth2 { .. }
        ));
    }
}
