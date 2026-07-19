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
// The leaf value newtypes live in a sibling module (file-size split); re-export
// them here so `oauth::{HttpsUrl, OAuthClientId, OAuthScopes}` stays the public
// path and this module can use them unqualified.
pub use crate::oauth_values::{HttpsUrl, OAuthClientId, OAuthScopes};

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
    /// Provider-specific parameters appended only to the **authorization**
    /// request (never the token request), e.g. Google's `access_type=offline`
    /// and `prompt=consent`, which are what make Google return a refresh token.
    pub extra_auth_params: Vec<AuthParam>,
}

impl OAuthProvider {
    /// Returns the built-in endpoints, scopes, and authorization parameters for
    /// a known provider, or `None` for [`OAuthProvider::Custom`].
    ///
    /// # Panics
    ///
    /// Never: the literals below are validated by the type constructors and a
    /// failure would be a compile-time-constant programmer error surfaced by
    /// the unit tests, not a runtime condition.
    #[must_use]
    pub fn preset(self) -> Option<OAuthPreset> {
        let build = |auth: &str, token: &str, scopes: &[&str], extra: &[(&str, &str)]| {
            Some(OAuthPreset {
                auth_url: HttpsUrl::parse(auth).ok()?,
                token_url: HttpsUrl::parse(token).ok()?,
                scopes: OAuthScopes::new(scopes.iter().map(|s| (*s).to_owned()).collect()).ok()?,
                extra_auth_params: extra
                    .iter()
                    .map(|(key, value)| AuthParam::parse(*key, *value).ok())
                    .collect::<Option<Vec<_>>>()?,
            })
        };
        match self {
            Self::Gmail => build(
                "https://accounts.google.com/o/oauth2/v2/auth",
                "https://oauth2.googleapis.com/token",
                &["https://mail.google.com/"],
                // Google only returns a refresh token when offline access is
                // requested, and only re-returns one on re-consent, so both are
                // required for a durable XOAUTH2 login.
                &[("access_type", "offline"), ("prompt", "consent")],
            ),
            Self::Microsoft => build(
                "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
                "https://login.microsoftonline.com/common/oauth2/v2.0/token",
                // `offline_access` is the scope that yields a (rotating)
                // refresh token from Microsoft identity platform.
                &[
                    "https://outlook.office.com/IMAP.AccessAsUser.All",
                    "https://outlook.office.com/SMTP.Send",
                    "offline_access",
                ],
                &[],
            ),
            Self::Custom => None,
        }
    }

    /// The provider whose preset authorization endpoint exactly equals
    /// `auth_url`, if any. Used to attach a provider's authorization
    /// parameters to a consent URL built from a persisted config, so the same
    /// parameters apply on both first sign-in and re-authorization.
    #[must_use]
    pub fn from_auth_url(auth_url: &str) -> Option<Self> {
        [Self::Gmail, Self::Microsoft].into_iter().find(|provider| {
            provider
                .preset()
                .is_some_and(|p| p.auth_url.as_str() == auth_url)
        })
    }
}

/// Returns the provider-specific authorization-request parameters for the
/// account whose authorization endpoint is `auth_url`, or an empty list when
/// the endpoint does not match a known preset (a custom provider).
///
/// This keeps the parameter set as a property of the [`OAuthPreset`] model
/// rather than as scheme-detecting string logic in the URL builder.
#[must_use]
pub fn extra_auth_params_for(auth_url: &str) -> Vec<AuthParam> {
    OAuthProvider::from_auth_url(auth_url)
        .and_then(OAuthProvider::preset)
        .map(|preset| preset.extra_auth_params)
        .unwrap_or_default()
}

/// A single provider-specific authorization-request parameter, such as
/// `access_type=offline`.
///
/// These are non-secret, static per provider, and defined only by the built-in
/// [`OAuthPreset`]s. The key and value are validated free of whitespace and
/// control characters so they cannot corrupt the query string they are
/// percent-encoded into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthParam {
    key: String,
    value: String,
}

impl AuthParam {
    /// Validates and wraps an authorization parameter.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] for an empty key, or a key/value
    /// containing whitespace or control characters.
    pub fn parse(key: impl Into<String>, value: impl Into<String>) -> Result<Self, MailError> {
        let key = key.into();
        let value = value.into();
        let invalid = |message: &str| MailError::Configuration {
            message: format!("invalid OAuth authorization parameter: {message}"),
        };
        if key.is_empty() {
            return Err(invalid("empty key"));
        }
        if key.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(invalid("key contains whitespace"));
        }
        if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(invalid("value contains whitespace"));
        }
        Ok(Self { key, value })
    }

    /// The parameter name.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The parameter value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
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
    fn gmail_preset_requests_offline_access_on_the_authorization_request() {
        let gmail = OAuthProvider::Gmail.preset().unwrap();
        let params: Vec<(&str, &str)> = gmail
            .extra_auth_params
            .iter()
            .map(|p| (p.key(), p.value()))
            .collect();
        assert!(params.contains(&("access_type", "offline")));
        assert!(params.contains(&("prompt", "consent")));
    }

    #[test]
    fn microsoft_preset_carries_offline_access_scope_and_no_extra_auth_params() {
        let ms = OAuthProvider::Microsoft.preset().unwrap();
        assert!(ms.scopes.as_slice().iter().any(|s| s == "offline_access"));
        assert!(ms.extra_auth_params.is_empty());
    }

    #[test]
    fn extra_auth_params_resolve_by_authorization_endpoint() {
        // Gmail's endpoint resolves to its offline-access parameters.
        let gmail_url = OAuthProvider::Gmail.preset().unwrap().auth_url;
        let params = extra_auth_params_for(gmail_url.as_str());
        assert_eq!(params.len(), 2);
        // Microsoft has no extra authorization parameters.
        let ms_url = OAuthProvider::Microsoft.preset().unwrap().auth_url;
        assert!(extra_auth_params_for(ms_url.as_str()).is_empty());
        // An unknown (custom) endpoint has none.
        assert!(extra_auth_params_for("https://auth.custom.example/authorize").is_empty());
    }

    #[test]
    fn auth_param_rejects_whitespace_and_empty_key() {
        assert!(AuthParam::parse("", "v").is_err());
        assert!(AuthParam::parse("k", "has space").is_err());
        assert!(AuthParam::parse("has space", "v").is_err());
        let param = AuthParam::parse("access_type", "offline").unwrap();
        assert_eq!((param.key(), param.value()), ("access_type", "offline"));
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
