//! Error type for the local test-server infrastructure.
//!
//! This crate is a dev-dependency only, so it keeps its own lightweight
//! error rather than pulling in `thiserror`. Every fallible operation in
//! the crate returns [`TestServerError`].

use std::fmt;

/// Failure raised while generating a test identity or running a scripted
/// server dialog.
#[derive(Debug)]
#[non_exhaustive]
pub enum TestServerError {
    /// Underlying socket or stream I/O failed.
    Io(std::io::Error),
    /// TLS setup or handshake failed.
    Tls(String),
    /// Self-signed certificate generation failed.
    CertGen(String),
    /// A scripted expectation was not satisfied by the peer.
    Script(String),
}

impl fmt::Display for TestServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "test server I/O error: {err}"),
            Self::Tls(message) => write!(f, "test server TLS error: {message}"),
            Self::CertGen(message) => {
                write!(f, "test certificate generation failed: {message}")
            }
            Self::Script(message) => write!(f, "scripted dialog expectation failed: {message}"),
        }
    }
}

impl std::error::Error for TestServerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TestServerError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}
