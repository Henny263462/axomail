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

/// The user's preferred UI language.
///
/// `System` follows the operating-system/browser language (resolved to a
/// concrete locale in the frontend i18n layer); `En` and `De` pin one. Like
/// the color theme it is applied entirely in the frontend, but unlike the
/// theme it is carried by its own `get_language` / `set_language` command pair
/// rather than [`SettingsDto`], so it does not widen the batch-saved surface.
/// Serialized in `lowercase` (`"system"`, `"en"`, `"de"`) to match the
/// frontend `LanguagePreference` union exactly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguagePreference {
    /// Follow the OS/browser language (the default).
    #[default]
    System,
    /// Always use English (the source locale).
    En,
    /// Always use German.
    De,
}

impl LanguagePreference {
    /// The canonical string stored in the `app_settings` table. Kept in sync
    /// with the `serde` `lowercase` renaming so persisted and serialized forms
    /// are identical.
    #[must_use]
    pub fn as_key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::En => "en",
            Self::De => "de",
        }
    }

    /// Parses the stored key, falling back to [`LanguagePreference::System`]
    /// for an absent or unrecognized value so a corrupt row can never brick
    /// the UI.
    #[must_use]
    pub fn from_key(value: &str) -> Self {
        match value {
            "en" => Self::En,
            "de" => Self::De,
            _ => Self::System,
        }
    }
}

/// How incoming read-receipt (MDN) requests are honoured.
///
/// `Never` never sends a receipt; `Always` sends one automatically; `Ask`
/// surfaces the request in the reading pane and waits for the user to decide.
/// The two conditions in [`ReceiptSettingsDto`] can downgrade `Ask`/`Always` to
/// "do not send" for a specific message. Serialized in `lowercase`
/// (`"never"`, `"ask"`, `"always"`) to match the frontend union.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReceiptPolicy {
    /// Never send a read receipt.
    Never,
    /// Ask before sending (the default): show the request, act on confirmation.
    #[default]
    Ask,
    /// Send a read receipt automatically.
    Always,
}

impl ReceiptPolicy {
    /// The canonical string stored in the `app_settings` table.
    #[must_use]
    pub fn as_key(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Ask => "ask",
            Self::Always => "always",
        }
    }

    /// Parses the stored key, falling back to [`ReceiptPolicy::Ask`] for an
    /// absent or unrecognized value (the safe middle ground: never auto-sends,
    /// never silently suppresses).
    #[must_use]
    pub fn from_key(value: &str) -> Self {
        match value {
            "never" => Self::Never,
            "always" => Self::Always,
            _ => Self::Ask,
        }
    }
}

/// The read-receipt (MDN) honour policy plus its two conditions.
///
/// `onlyWhenAddressed`: when `true`, only honour a request for a message that
/// has one of my addresses in `To`/`Cc`. `neverForMailingLists`: when `true`,
/// never send a receipt for list/bulk mail. Both are privacy-preserving and on
/// by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptSettingsDto {
    /// Base policy for honouring a receipt request.
    pub policy: ReceiptPolicy,
    /// Only honour a request when the message is addressed to me.
    pub only_when_addressed: bool,
    /// Never honour a request for mailing-list / bulk mail.
    pub never_for_mailing_lists: bool,
}

impl Default for ReceiptSettingsDto {
    fn default() -> Self {
        Self {
            policy: ReceiptPolicy::default(),
            only_when_addressed: true,
            never_for_mailing_lists: true,
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
    /// Whether a displayed message is automatically marked read.
    pub auto_mark_read_enabled: bool,
    /// Delay before auto-marking a displayed message read, in milliseconds
    /// (`0` = immediate; clamped to a ceiling on write).
    pub auto_mark_read_delay_ms: u32,
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
    fn language_default_is_system() {
        assert_eq!(LanguagePreference::default(), LanguagePreference::System);
    }

    #[test]
    fn language_key_round_trips_every_variant() {
        for variant in [
            LanguagePreference::System,
            LanguagePreference::En,
            LanguagePreference::De,
        ] {
            assert_eq!(LanguagePreference::from_key(variant.as_key()), variant);
        }
    }

    #[test]
    fn language_from_unknown_key_falls_back_to_system() {
        assert_eq!(LanguagePreference::from_key(""), LanguagePreference::System);
        assert_eq!(
            LanguagePreference::from_key("fr"),
            LanguagePreference::System
        );
    }

    #[test]
    fn language_serializes_as_lowercase_matching_the_stored_key() {
        for variant in [
            LanguagePreference::System,
            LanguagePreference::En,
            LanguagePreference::De,
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
            auto_mark_read_enabled: true,
            auto_mark_read_delay_ms: 0,
            pop3_delete_policies: Vec::new(),
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"theme\":\"dark\""), "got {json}");
        assert!(json.contains("\"autoMarkReadDelayMs\":0"), "got {json}");
    }
}
