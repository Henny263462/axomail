//! DTOs for the per-mailbox retention policy shown in the folder-properties
//! dialog, and the outcome of a maintenance pass.

use axomail_core::MailError;
use axomail_storage::{RetentionAction, RetentionMode, RetentionPolicy};
use serde::{Deserialize, Serialize};

use super::checked_id;

/// A mailbox's retention policy for display.
///
/// `mode` serializes as `"keep_all"`/`"by_age"`/`"by_count"` and `action` as
/// `"trash"`/`"purge_body"` (mirroring the storage enums). `days` applies to
/// `by_age`, `maxCount` to `by_count`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionPolicyDto {
    /// Mailbox the policy belongs to.
    pub mailbox_id: i64,
    /// Which messages the mailbox keeps.
    pub mode: RetentionMode,
    /// Age boundary (days) for `by_age`.
    pub days: u32,
    /// Keep-count for `by_count`.
    pub max_count: u32,
    /// How over-retention messages are disposed of.
    pub action: RetentionAction,
}

impl RetentionPolicyDto {
    /// Builds the DTO for a mailbox, validating the id crosses to JavaScript
    /// safely.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if `mailbox_id` is out of the safe
    /// integer range.
    pub fn new(mailbox_id: i64, policy: RetentionPolicy) -> Result<Self, MailError> {
        Ok(Self {
            mailbox_id: checked_id(mailbox_id)?,
            mode: policy.mode,
            days: policy.days,
            max_count: policy.max_count,
            action: policy.action,
        })
    }
}

/// The editable retention fields sent from the folder-properties dialog.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionPolicyInput {
    /// Which messages the mailbox keeps.
    pub mode: RetentionMode,
    /// Age boundary (days) for `by_age`.
    pub days: u32,
    /// Keep-count for `by_count`.
    pub max_count: u32,
    /// How over-retention messages are disposed of.
    pub action: RetentionAction,
}

impl RetentionPolicyInput {
    /// Resolves the input into a validated [`RetentionPolicy`], clamping the
    /// numeric bounds to at least one so a zero can never disable the CHECK.
    #[must_use]
    pub fn into_policy(self) -> RetentionPolicy {
        RetentionPolicy {
            mode: self.mode,
            days: self.days.max(1),
            max_count: self.max_count.max(1),
            action: self.action,
        }
    }
}

/// What a retention maintenance pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionOutcomeDto {
    /// How many messages were moved to Trash.
    pub moved_to_trash: u64,
    /// How many messages had their local cached body purged.
    pub purged_bodies: u64,
}
