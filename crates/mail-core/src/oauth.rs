//! OAuth2 / XOAUTH2 configuration for an account.
//!
//! Everything in this module is **non-secret**: the client id, the authorization
//! and token endpoint URLs, and the requested scopes. The tokens themselves
//! (access + refresh) live only in the OS keychain via `axomail-security` and
//! never appear in these types, in the database, or in logs (ADR-0001).
//!
//! axomail ships **no** registered OAuth client id. PKCE public-client flows
//! (RFC 8252) are used, so there is no client secret anywhere; the user
//! supplies a client id registered in their own provider project.

use serde::{Deserialize, Serialize};

use crate::error::MailError;

/// How an account authenticates to its servers.
///
/// Mirrors the `accounts.auth_mechanism` column CHECK constraint
/// (`'password'` / `'oauth2'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMechanism {
    /// A password stored in the keychain, presented via LOGIN/AUTH PLAIN.
    Password,
    /// An OAuth2 access token, presented via the XOAUTH2 SASL mechanism.
    OAuth2,
}

/// A known OAuth2 provider preset, or a fully custom endpoint set.
///
/// The preset only pre-fills the endpoints and scopes in the setup UI; once an
/// account is stored, its endpoints and scopes are authoritative and this type
/// is no longer consulted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OAuthProvider {
    /// Google / Gmail.
    Gmail,
    /// Microsoft 365 / Outlook.com.
    Microsoft,
    /// User-supplied endpoints and scopes.
    Custom,
}

/// The endpoint/scope preset for a well-known provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthPreset {
    /// Authorization endpoint (the consent URL base).
    pub auth_url: HttpsUrl,
    /// Token endpoint (code exchange and refresh).
    pub token_url: HttpsUrl,
    /// Scopes granting IMAP/SMTP (and offline access for a refresh token).
    pub scopes: OAuthScopes,
}

impl OAuthProvider {
    /// Returns the built-in endpoints and scopes for a known provider, or
    /// `None` for [`OAuthProvider::Custom`].
    ///
    /// # Panics
    ///
    /// Never: the literals below are validated by the type constructors and a
    /// failure would be a compile-time-constant programmer error surfaced by
    /// the unit tests, not a runtime condition.
    #[must_use]
    pub fn preset(self) -> Option<OAuthPreset> {
        let build = |auth: &str, token: &str, scopes: &[&str]| {
            Some(OAuthPreset {
                auth_url: HttpsUrl::parse(auth).ok()?,
                token_url: HttpsUrl::parse(token).ok()?,
                scopes: OAuthScopes::new(scopes.iter().map(|s| (*s).to_owned()).collect()).ok()?,
            })
        };
        match self {
            Self::Gmail => build(
                "https://accounts.google.com/o/oauth2/v2/auth",
                "https://oauth2.googleapis.com/token",
                &["https://mail.google.com/"],
            ),
            Self::Microsoft => build(
                "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
                "https://login.microsoftonline.com/common/oauth2/v2.0/token",
                &[
                    "https://outlook.office.com/IMAP.AccessAsUser.All",
                    "https://outlook.office.com/SMTP.Send",
                    "offline_access",
                ],
            ),
            Self::Custom => None,
        }
    }
}

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

/// The complete, non-secret OAuth2 configuration persisted for an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// User-supplied public client id.
    pub client_id: OAuthClientId,
    /// Authorization (consent) endpoint.
    pub auth_url: HttpsUrl,
    /// Token endpoint (code exchange and refresh).
    pub token_url: HttpsUrl,
    /// Requested scopes.
    pub scopes: OAuthScopes,
}

/// How an account authenticates, with the OAuth2 endpoints when applicable.
///
/// This is the model-level counterpart of [`AuthMechanism`]; the connection
/// layer resolves it to a [`ConnectionCredential`](crate::ConnectionCredential)
/// (a password or a freshly refreshed access token) at connect time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountAuth {
    /// Password authentication; the secret lives under the account's
    /// credential reference in the keychain.
    Password,
    /// OAuth2 authentication with the given endpoints; the access and refresh
    /// tokens live under the account's credential reference in the keychain.
    OAuth2(OAuthConfig),
}

impl AccountAuth {
    /// The default authentication mode (password), used by serde when the
    /// field is absent from a legacy payload.
    #[must_use]
    pub fn default_password() -> Self {
        Self::Password
    }

    /// Returns the mechanism discriminant.
    #[must_use]
    pub fn mechanism(&self) -> AuthMechanism {
        match self {
            Self::Password => AuthMechanism::Password,
            Self::OAuth2(_) => AuthMechanism::OAuth2,
        }
    }

    /// Returns the OAuth2 configuration when this is an OAuth2 account.
    #[must_use]
    pub fn oauth(&self) -> Option<&OAuthConfig> {
        match self {
            Self::Password => None,
            Self::OAuth2(config) => Some(config),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn auth_mechanism_serializes_as_check_tokens() {
        assert_eq!(
            serde_json::to_string(&AuthMechanism::Password).unwrap(),
            "\"password\""
        );
        assert_eq!(
            serde_json::to_string(&AuthMechanism::OAuth2).unwrap(),
            "\"oauth2\""
        );
    }

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

    #[test]
    fn known_presets_are_valid_and_custom_is_none() {
        assert!(OAuthProvider::Gmail.preset().is_some());
        assert!(OAuthProvider::Microsoft.preset().is_some());
        assert!(OAuthProvider::Custom.preset().is_none());
        let gmail = OAuthProvider::Gmail.preset().unwrap();
        assert_eq!(
            gmail.token_url.as_str(),
            "https://oauth2.googleapis.com/token"
        );
    }

    #[test]
    fn oauth_config_round_trips_through_serde_with_validation() {
        let config = OAuthConfig {
            client_id: OAuthClientId::parse("client-1").unwrap(),
            auth_url: HttpsUrl::parse("https://auth.example.org/a").unwrap(),
            token_url: HttpsUrl::parse("https://auth.example.org/t").unwrap(),
            scopes: OAuthScopes::new(vec!["scope-a".into()]).unwrap(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: OAuthConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
        // A tampered URL is rejected on deserialize.
        let bad = json.replace("https://auth.example.org/t", "http://evil.example.org");
        assert!(serde_json::from_str::<OAuthConfig>(&bad).is_err());
    }

    #[test]
    fn account_auth_reports_mechanism_and_config() {
        assert_eq!(AccountAuth::Password.mechanism(), AuthMechanism::Password);
        assert!(AccountAuth::Password.oauth().is_none());
        let auth = AccountAuth::OAuth2(OAuthConfig {
            client_id: OAuthClientId::parse("c").unwrap(),
            auth_url: HttpsUrl::parse("https://a.example.org").unwrap(),
            token_url: HttpsUrl::parse("https://t.example.org").unwrap(),
            scopes: OAuthScopes::new(vec!["s".into()]).unwrap(),
        });
        assert_eq!(auth.mechanism(), AuthMechanism::OAuth2);
        assert!(auth.oauth().is_some());
    }
}
