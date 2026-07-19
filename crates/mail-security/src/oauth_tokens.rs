//! The [`OAuthTokens`] keychain payload.
//!
//! OAuth2 access and refresh tokens are secrets: like passwords, they live
//! only in the OS keychain, referenced by a non-secret [`CredentialRef`], and
//! never touch the database or logs (ADR-0001). This type is the JSON payload
//! stored under an account's credential reference when the account uses
//! OAuth2. Its [`Debug`] implementation is hand-written to redact both tokens,
//! so a stray `{:?}` cannot leak them across a DTO or trace boundary.

use axomail_core::MailError;
use serde::{Deserialize, Serialize};

/// A stored OAuth2 token set: the current access token, the refresh token
/// used to mint new access tokens, and the absolute expiry of the access
/// token as a Unix timestamp (seconds).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthTokens {
    /// The bearer access token presented to the mail servers via XOAUTH2.
    pub access_token: String,
    /// The refresh token used to obtain a new access token. May be empty when
    /// a provider does not rotate or re-issue one on refresh.
    pub refresh_token: String,
    /// Absolute expiry of `access_token` as a Unix timestamp (seconds).
    pub expires_at_unix: i64,
}

impl OAuthTokens {
    /// Serializes the token set to the JSON stored in the keychain.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Keychain`] if serialization fails (which would
    /// indicate a programmer error, not a runtime condition).
    pub fn to_json(&self) -> Result<String, MailError> {
        serde_json::to_string(self).map_err(|_| MailError::Keychain {
            message: "failed to serialize OAuth tokens".to_owned(),
        })
    }

    /// Parses a token set from the JSON stored in the keychain.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Keychain`] if the payload is not the expected
    /// shape. The raw payload is never echoed into the error (it is secret).
    pub fn from_json(payload: &str) -> Result<Self, MailError> {
        serde_json::from_str(payload).map_err(|_| MailError::Keychain {
            message: "stored OAuth token payload is not valid".to_owned(),
        })
    }

    /// Returns `true` when the access token is expired or will expire within
    /// `skew_secs` of `now_unix` (a margin so a token is refreshed slightly
    /// early rather than mid-request).
    #[must_use]
    pub fn is_expired(&self, now_unix: i64, skew_secs: i64) -> bool {
        self.expires_at_unix <= now_unix.saturating_add(skew_secs)
    }
}

impl std::fmt::Debug for OAuthTokens {
    /// Redacts both tokens; only the (non-secret) expiry is printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthTokens")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("expires_at_unix", &self.expires_at_unix)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn sample() -> OAuthTokens {
        OAuthTokens {
            access_token: "ya29.access-secret".to_owned(),
            refresh_token: "1//refresh-secret".to_owned(),
            expires_at_unix: 1_000_000,
        }
    }

    #[test]
    fn json_round_trips() {
        let tokens = sample();
        let json = tokens.to_json().unwrap();
        let back = OAuthTokens::from_json(&json).unwrap();
        assert_eq!(tokens, back);
    }

    #[test]
    fn invalid_payload_is_rejected_without_leaking() {
        let err = OAuthTokens::from_json("{ not json").unwrap_err();
        let rendered = err.to_string();
        assert!(!rendered.contains("not json"));
    }

    #[test]
    fn debug_redacts_both_tokens() {
        let rendered = format!("{:?}", sample());
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("ya29.access-secret"));
        assert!(!rendered.contains("1//refresh-secret"));
        assert!(rendered.contains("1000000"));
    }

    #[test]
    fn expiry_accounts_for_clock_skew() {
        let tokens = sample();
        // Not yet due well before expiry.
        assert!(!tokens.is_expired(900_000, 60));
        // Due once within the skew window of expiry.
        assert!(tokens.is_expired(999_950, 60));
        // Due after expiry.
        assert!(tokens.is_expired(1_000_001, 0));
    }
}
