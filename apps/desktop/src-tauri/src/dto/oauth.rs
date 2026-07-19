//! DTOs for the OAuth2 browser sign-in command.

use serde::Serialize;

/// Outcome of the browser sign-in flow, reported to the setup UI.
///
/// Only a status crosses the boundary; tokens never do. `denied` and
/// `timedOut` are normal user outcomes, not errors, so the command returns
/// them in a successful result rather than as a rejected promise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OAuthAuthorizeStatus {
    /// The user consented and tokens were stored.
    Authorized,
    /// The user declined consent.
    Denied,
    /// No redirect arrived before the deadline.
    TimedOut,
}

/// The result payload of `oauth_authorize`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthAuthorizeResult {
    /// The sign-in outcome.
    pub status: OAuthAuthorizeStatus,
}

impl OAuthAuthorizeResult {
    /// Builds a result with the given status.
    #[must_use]
    pub fn new(status: OAuthAuthorizeStatus) -> Self {
        Self { status }
    }
}
