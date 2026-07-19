//! DTOs for the per-mailbox offline policy shown in the folder-properties
//! dialog.

use axomail_core::MailError;
use axomail_storage::{OfflineMode, OfflinePolicy};
use serde::{Deserialize, Serialize};

use super::checked_id;

/// A mailbox's offline policy for display and edit.
///
/// `mode` serializes as `"none"`/`"recent"`/`"all"` (mirroring
/// [`OfflineMode`]); `recentDays` is the age boundary honoured only in
/// `"recent"` mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflinePolicyDto {
    /// Mailbox the policy belongs to.
    pub mailbox_id: i64,
    /// How much of the mailbox is kept available offline.
    pub mode: OfflineMode,
    /// Age boundary (days) for `recent` mode.
    pub recent_days: u32,
}

impl OfflinePolicyDto {
    /// Builds the DTO for a mailbox, validating the id crosses to JavaScript
    /// safely.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if `mailbox_id` is out of the safe
    /// integer range.
    pub fn new(mailbox_id: i64, policy: OfflinePolicy) -> Result<Self, MailError> {
        Ok(Self {
            mailbox_id: checked_id(mailbox_id)?,
            mode: policy.mode,
            recent_days: policy.recent_days,
        })
    }
}
