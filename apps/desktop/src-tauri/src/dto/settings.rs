//! Application-settings DTOs exchanged with the settings view.

use axomail_core::MailError;
use serde::{Deserialize, Serialize};

use super::checked_id;

/// The user's preferred application color theme.
///
/// `System` follows the operating system's light/dark preference; `Light`
/// and `Dark` pin one regardless of the OS. Serialized in `lowercase`
/// (`"system"`, `"light"`, `"dark"`) to match the frontend `ThemePreference`
/// union exactly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    /// Follow the OS light/dark setting (the default).
    #[default]
    System,
    /// Always use the light palette.
    Light,
    /// Always use the dark palette.
    Dark,
}

impl ThemePreference {
    /// The canonical string stored in the `app_settings` table. Kept in sync
    /// with the `serde` `lowercase` renaming so persisted and serialized forms
    /// are identical.
    #[must_use]
    pub fn as_key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// Parses the stored key, falling back to [`ThemePreference::System`] for
    /// an absent or unrecognized value so a corrupt row can never brick the UI.
    #[must_use]
    pub fn from_key(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }
}

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
    /// Preferred application color theme (applied entirely in the frontend).
    pub theme: ThemePreference,
    /// Per-account POP3 server-deletion policies (display only on write).
    pub pop3_delete_policies: Vec<Pop3DeletePolicyDto>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn theme_default_is_system() {
        assert_eq!(ThemePreference::default(), ThemePreference::System);
    }

    #[test]
    fn theme_key_round_trips_every_variant() {
        for variant in [
            ThemePreference::System,
            ThemePreference::Light,
            ThemePreference::Dark,
        ] {
            assert_eq!(ThemePreference::from_key(variant.as_key()), variant);
        }
    }

    #[test]
    fn theme_from_unknown_key_falls_back_to_system() {
        assert_eq!(ThemePreference::from_key(""), ThemePreference::System);
        assert_eq!(
            ThemePreference::from_key("solarized"),
            ThemePreference::System
        );
    }

    #[test]
    fn theme_serializes_as_lowercase_matching_the_stored_key() {
        // The JSON form the frontend receives must equal the persisted key so
        // the two representations never drift.
        for variant in [
            ThemePreference::System,
            ThemePreference::Light,
            ThemePreference::Dark,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, format!("\"{}\"", variant.as_key()));
        }
    }

    #[test]
    fn settings_dto_uses_camel_case_theme_field() {
        let dto = SettingsDto {
            notifications_enabled: true,
            sync_interval_minutes: 10,
            theme: ThemePreference::Dark,
            pop3_delete_policies: Vec::new(),
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"theme\":\"dark\""), "got {json}");
    }
}
