//! DTO for the junk settings surface.

use axomail_storage::JunkSettings;
use serde::{Deserialize, Serialize};

/// The junk controls exposed to the settings section. Carries no ids, so no
/// [`checked_id`](crate::dto::checked_id) guard is needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JunkSettingsDto {
    /// Whether inbound scoring and auto-routing are active.
    pub enabled: bool,
    /// Score at or above which a message is treated as junk (`0.0..=1.0`).
    pub threshold: f64,
    /// Whether a server's `X-Spam-*` verdict is honored (run-controls path).
    pub trust_server_headers: bool,
    /// Whether address-book senders are exempt from junking (track 4.1).
    pub whitelist_contacts: bool,
}

impl JunkSettingsDto {
    /// Builds the DTO from the stored settings.
    #[must_use]
    pub fn from_settings(settings: JunkSettings) -> Self {
        Self {
            enabled: settings.enabled,
            threshold: settings.threshold,
            trust_server_headers: settings.trust_server_headers,
            whitelist_contacts: settings.whitelist_contacts,
        }
    }

    /// Converts the DTO to stored settings, clamping the threshold.
    #[must_use]
    pub fn into_settings(self) -> JunkSettings {
        JunkSettings {
            enabled: self.enabled,
            threshold: self.threshold.clamp(0.0, 1.0),
            trust_server_headers: self.trust_server_headers,
            whitelist_contacts: self.whitelist_contacts,
        }
    }
}
