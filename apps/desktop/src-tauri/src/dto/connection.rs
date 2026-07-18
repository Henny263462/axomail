//! Connection-test result DTO.

use serde::Serialize;

/// Outcome of probing an account's incoming and outgoing servers.
///
/// The two directions are reported independently so the setup UI can tell
/// the user precisely which server failed. Error strings are the redacted,
/// user-facing `Display` form of a [`axomail_core::MailError`]; they never
/// carry credentials or raw server responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    /// Whether the incoming server accepted the connection and login.
    pub incoming_ok: bool,
    /// Redacted reason the incoming probe failed, if it did.
    pub incoming_error: Option<String>,
    /// Whether the outgoing server probe succeeded.
    pub outgoing_ok: bool,
    /// Redacted reason the outgoing probe failed, if it did.
    pub outgoing_error: Option<String>,
}
