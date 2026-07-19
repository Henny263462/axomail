//! Central error hierarchy (ADR-0003).
//!
//! Every fallible core operation returns [`MailError`]. Variants carry
//! human-readable context but **never secrets**: no passwords, tokens,
//! authentication headers, or raw server responses that could embed
//! them. Adapters must redact protocol data before constructing errors.

use std::time::Duration;

use thiserror::Error;

/// Classification of an error for retry and UI decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorClass {
    /// Retrying with backoff may succeed (network hiccup, rate limit).
    Temporary,
    /// Retrying without intervention will not help.
    Permanent,
    /// The user must fix account or application configuration.
    UserConfiguration,
}

/// Central error type of the mail core.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MailError {
    /// Connection-level network failure (reset, refused, unreachable).
    #[error("network error: {message}")]
    Network {
        /// Redacted description of the failure.
        message: String,
    },

    /// Host name resolution failed.
    ///
    /// Adapters should map a definitive "name does not exist" answer to
    /// [`MailError::Configuration`] instead, since that usually means a
    /// mistyped server name.
    #[error("DNS resolution failed for {host}")]
    Dns {
        /// Host name that could not be resolved.
        host: String,
    },

    /// TLS handshake or certificate validation failure.
    ///
    /// Never retried automatically and never downgraded: certificate
    /// errors must surface to the user unchanged.
    #[error("TLS error: {message}")]
    Tls {
        /// Redacted description of the failure.
        message: String,
    },

    /// The server rejected the presented credentials.
    #[error("authentication rejected by the server")]
    Authentication,

    /// The peer violated the protocol or sent an unexpected response.
    #[error("protocol error: {message}")]
    Protocol {
        /// Redacted description of the violation.
        message: String,
    },

    /// The server signalled throttling.
    #[error("server rate limit hit")]
    RateLimited {
        /// Server-suggested wait time, if provided.
        retry_after: Option<Duration>,
    },

    /// An operation exceeded its deadline.
    #[error("operation timed out after {after:?}")]
    Timeout {
        /// Configured deadline that was exceeded.
        after: Duration,
    },

    /// Message content could not be parsed.
    #[error("failed to parse message content: {message}")]
    Parse {
        /// Redacted description of the parse failure.
        message: String,
    },

    /// Local database failure.
    #[error("database error: {message}")]
    Database {
        /// Redacted description of the failure.
        message: String,
    },

    /// Operating-system keychain access failed.
    #[error("keychain access failed: {message}")]
    Keychain {
        /// Redacted description of the failure.
        message: String,
    },

    /// Local filesystem failure (attachment store, logs, exports).
    #[error("filesystem error: {message}")]
    Filesystem {
        /// Redacted description of the failure.
        message: String,
    },

    /// The server lacks a capability the operation requires.
    #[error("server does not support required capability {capability}")]
    UnsupportedCapability {
        /// Name of the missing capability (e.g. `UIDL`, `STARTTLS`).
        capability: String,
    },

    /// User-supplied configuration is invalid.
    #[error("invalid configuration: {message}")]
    Configuration {
        /// Description of what is wrong and which field caused it.
        message: String,
    },

    /// A code path is a deliberate not-yet-implemented placeholder.
    ///
    /// Used by the sync engine's per-kind executor seams (Discover, OpReplay)
    /// until the Wave 2 tracks drop in their real executors. Classified as
    /// [`ErrorClass::Permanent`] so retry logic never spins on a stub.
    #[error("not yet implemented: {feature}")]
    NotImplemented {
        /// Which feature is not yet wired, plus its owning track.
        feature: String,
    },

    /// The operation was cancelled on request.
    ///
    /// Classified as [`ErrorClass::Permanent`] so retry logic never
    /// resurrects cancelled work.
    #[error("operation was cancelled")]
    Cancelled,
}

impl MailError {
    /// Classifies the error for retry and presentation decisions.
    #[must_use]
    pub fn class(&self) -> ErrorClass {
        match self {
            Self::Network { .. }
            | Self::Dns { .. }
            | Self::RateLimited { .. }
            | Self::Timeout { .. } => ErrorClass::Temporary,
            Self::Tls { .. }
            | Self::Protocol { .. }
            | Self::Parse { .. }
            | Self::Database { .. }
            | Self::Keychain { .. }
            | Self::Filesystem { .. }
            | Self::UnsupportedCapability { .. }
            | Self::NotImplemented { .. }
            | Self::Cancelled => ErrorClass::Permanent,
            Self::Authentication | Self::Configuration { .. } => ErrorClass::UserConfiguration,
        }
    }

    /// Returns `true` if retrying with backoff is reasonable.
    #[must_use]
    pub fn is_temporary(&self) -> bool {
        self.class() == ErrorClass::Temporary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_matches_retry_semantics() {
        let cases: Vec<(MailError, ErrorClass)> = vec![
            (
                MailError::Network {
                    message: "connection reset".into(),
                },
                ErrorClass::Temporary,
            ),
            (
                MailError::Dns {
                    host: "imap.example.org".into(),
                },
                ErrorClass::Temporary,
            ),
            (
                MailError::RateLimited { retry_after: None },
                ErrorClass::Temporary,
            ),
            (
                MailError::Timeout {
                    after: Duration::from_secs(30),
                },
                ErrorClass::Temporary,
            ),
            (
                MailError::Tls {
                    message: "certificate expired".into(),
                },
                ErrorClass::Permanent,
            ),
            (
                MailError::Protocol {
                    message: "unexpected greeting".into(),
                },
                ErrorClass::Permanent,
            ),
            (MailError::Cancelled, ErrorClass::Permanent),
            (MailError::Authentication, ErrorClass::UserConfiguration),
            (
                MailError::Configuration {
                    message: "port must not be 0".into(),
                },
                ErrorClass::UserConfiguration,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.class(), expected, "wrong class for {error:?}");
        }
    }

    #[test]
    fn temporary_helper_matches_class() {
        assert!(
            MailError::Timeout {
                after: Duration::from_secs(1)
            }
            .is_temporary()
        );
        assert!(!MailError::Authentication.is_temporary());
    }

    #[test]
    fn authentication_error_message_contains_no_credential_placeholder() {
        let rendered = MailError::Authentication.to_string();
        assert_eq!(rendered, "authentication rejected by the server");
    }
}
