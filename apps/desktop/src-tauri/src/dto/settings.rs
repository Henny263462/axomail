//! Application-settings DTOs exchanged with the settings view.

use axomail_core::MailError;
use serde::{Deserialize, Serialize};

use super::checked_id;

/// One account's POP3 server-deletion policy.
///
/// Surfaced read-only through [`SettingsDto`] so the settings view can show
/// the current state; it is *changed* only by the dedicated
/// `set_pop3_delete_policy` command, never by an `update_settings` call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pop3DeletePolicyDto {
    /// Account the policy applies to.
    pub account_id: i64,
    /// Whether downloaded messages are deleted from the server.
    pub enabled: bool,
}

impl Pop3DeletePolicyDto {
    /// Builds a policy entry, validating the account id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the account id is out of the
    /// safe integer range.
    pub fn new(account_id: i64, enabled: bool) -> Result<Self, MailError> {
        Ok(Self {
            account_id: checked_id(account_id)?,
            enabled,
        })
    }
}

/// User-editable application settings.
///
/// `syncIntervalMinutes` has a minimum of 1; the backend clamps it on write.
/// `pop3DeletePolicies` is read-only on the way in: `update_settings` ignores
/// it (the per-account command is the only way to change server deletion).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    /// Whether new-mail desktop notifications are shown.
    pub notifications_enabled: bool,
    /// Background-sync polling interval in whole minutes (clamped `>= 1`).
    pub sync_interval_minutes: u32,
    /// Per-account POP3 server-deletion policies (display only on write).
    pub pop3_delete_policies: Vec<Pop3DeletePolicyDto>,
}
