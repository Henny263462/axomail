//! Validated OAuth2 value types: the non-secret client id, endpoint URLs, and
//! scope set.
//!
//! These are the leaf value newtypes used across [`crate::oauth`]. Each
//! validates on construction and via serde `try_from`, so a deserialized
//! payload cannot bypass the invariants. None of them is a secret (ADR-0001):
//! the client id is a public identifier and the endpoints/scopes are
//! configuration, while the tokens themselves live only in the keychain.

use serde::{Deserialize, Serialize};

use crate::error::MailError;

/// A non-empty OAuth2 client id (a public identifier, never a secret).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OAuthClientId(String);

impl OAuthClientId {
    /// Validates and wraps a client id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] for an empty id or one containing
    /// whitespace or control characters.
    pub fn parse(value: impl Into<String>) -> Result<Self, MailError> {
        let value = value.into();
        if value.is_empty() {
            return Err(MailError::Configuration {
                message: "OAuth client id must not be empty".into(),
            });
        }
        if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(MailError::Configuration {
                message: "OAuth client id must not contain whitespace".into(),
            });
        }
        Ok(Self(value))
    }

    /// Returns the client id string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for OAuthClientId {
    type Error = MailError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<OAuthClientId> for String {
    fn from(value: OAuthClientId) -> Self {
        value.0
    }
}

/// An `https://` URL used as an OAuth2 endpoint.
///
/// Only the scheme and basic well-formedness are checked here; the transport's
/// TLS stack performs certificate validation at request time. Plain `http`
/// endpoints are rejected so a misconfiguration cannot send a token request in
/// the clear.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HttpsUrl(String);

impl HttpsUrl {
    /// Validates and wraps an `https://` URL.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] when the value is not an
    /// `https://` URL, contains whitespace or control characters, or has an
    /// empty host.
    pub fn parse(value: impl Into<String>) -> Result<Self, MailError> {
        let value = value.into();
        let invalid = |message: &str| MailError::Configuration {
            message: format!("invalid OAuth endpoint URL: {message}"),
        };
        if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(invalid("contains whitespace"));
        }
        let Some(rest) = value.strip_prefix("https://") else {
            return Err(invalid("must use the https scheme"));
        };
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        if host.is_empty() {
            return Err(invalid("empty host"));
        }
        Ok(Self(value))
    }

    /// Returns the URL string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for HttpsUrl {
    type Error = MailError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<HttpsUrl> for String {
    fn from(value: HttpsUrl) -> Self {
        value.0
    }
}

/// A non-empty set of OAuth2 scopes.
///
/// Each scope is a single space-free token; the wire form joins them with
/// spaces (RFC 6749 §3.3), so an embedded space would corrupt the request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<String>", into = "Vec<String>")]
pub struct OAuthScopes(Vec<String>);

impl OAuthScopes {
    /// Validates and wraps a list of scopes.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] when the list is empty or any
    /// scope is empty or contains whitespace or control characters.
    pub fn new(scopes: Vec<String>) -> Result<Self, MailError> {
        if scopes.is_empty() {
            return Err(MailError::Configuration {
                message: "at least one OAuth scope is required".into(),
            });
        }
        for scope in &scopes {
            if scope.is_empty() {
                return Err(MailError::Configuration {
                    message: "OAuth scope must not be empty".into(),
                });
            }
            if scope.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err(MailError::Configuration {
                    message: "OAuth scope must not contain whitespace".into(),
                });
            }
        }
        Ok(Self(scopes))
    }

    /// Returns the scopes as a single space-delimited string for the wire.
    #[must_use]
    pub fn as_space_delimited(&self) -> String {
        self.0.join(" ")
    }

    /// Returns the individual scopes.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

impl TryFrom<Vec<String>> for OAuthScopes {
    type Error = MailError;
    fn try_from(value: Vec<String>) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<OAuthScopes> for Vec<String> {
    fn from(value: OAuthScopes) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn client_id_rejects_blank_and_whitespace() {
        assert!(OAuthClientId::parse("").is_err());
        assert!(OAuthClientId::parse("has space").is_err());
        assert!(OAuthClientId::parse("123.apps.googleusercontent.com").is_ok());
    }

    #[test]
    fn https_url_requires_https_and_host() {
        assert!(HttpsUrl::parse("http://token.example.org/").is_err());
        assert!(HttpsUrl::parse("https:///path").is_err());
        assert!(HttpsUrl::parse("https://token.example.org/oauth?x=1").is_ok());
        assert!(HttpsUrl::parse("https://has space.example.org/").is_err());
    }

    #[test]
    fn scopes_reject_empty_list_and_embedded_space() {
        assert!(OAuthScopes::new(vec![]).is_err());
        assert!(OAuthScopes::new(vec!["a b".into()]).is_err());
        let scopes = OAuthScopes::new(vec!["mail".into(), "offline_access".into()]).unwrap();
        assert_eq!(scopes.as_space_delimited(), "mail offline_access");
    }
}
