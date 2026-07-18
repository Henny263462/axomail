//! Typed persistence of user settings over the `app_settings` key/value
//! table ([`axomail_storage::SettingsRepo`]).
//!
//! # Stored keys
//!
//! | key | value | default |
//! |-----|-------|---------|
//! | `notifications_enabled` | `"true"`/`"false"` | `true` |
//! | `sync_interval_minutes` | decimal integer, clamped `>= 1` | `10` |
//! | `pop3_delete_policy.<accountId>` | `"true"`/`"false"` | `false` |
//!
//! The POP3 delete policy is the second of the two guards against POP3's
//! destructive `DELE`: it is written **only** by the explicit
//! `set_pop3_delete_policy` command, never by account setup and never by a
//! bulk settings update, so server-side deletion can never be switched on as
//! a side effect.

use axomail_core::MailError;
use axomail_storage::Store;

/// Whether desktop notifications are shown when a sync brings new mail.
const KEY_NOTIFICATIONS: &str = "notifications_enabled";
/// Base background-sync polling interval, in whole minutes.
const KEY_SYNC_INTERVAL: &str = "sync_interval_minutes";
/// Prefix of the per-account POP3 server-deletion policy key.
const KEY_POP3_DELETE_PREFIX: &str = "pop3_delete_policy.";

/// Notifications are on by default; they only ever carry an aggregate count.
const DEFAULT_NOTIFICATIONS: bool = true;
/// Default polling interval when the user has not chosen one.
pub const DEFAULT_SYNC_INTERVAL_MINUTES: u32 = 10;
/// Hard floor on the polling interval; short-interval polling is a non-goal.
pub const MIN_SYNC_INTERVAL_MINUTES: u32 = 1;

/// Raises a requested interval to the one-minute floor.
#[must_use]
pub fn clamp_interval(minutes: u32) -> u32 {
    minutes.max(MIN_SYNC_INTERVAL_MINUTES)
}

/// Builds the per-account POP3 delete-policy key.
fn pop3_delete_key(account_id: i64) -> String {
    format!("{KEY_POP3_DELETE_PREFIX}{account_id}")
}

/// Reads whether new-mail notifications are enabled (default `true`).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn notifications_enabled(store: &Store) -> Result<bool, MailError> {
    Ok(store
        .settings()
        .get(KEY_NOTIFICATIONS)
        .await?
        .map_or(DEFAULT_NOTIFICATIONS, |value| value == "true"))
}

/// Reads the background-sync interval in minutes, clamped to the floor
/// (default [`DEFAULT_SYNC_INTERVAL_MINUTES`]).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn sync_interval_minutes(store: &Store) -> Result<u32, MailError> {
    let stored = store.settings().get(KEY_SYNC_INTERVAL).await?;
    let minutes = stored
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_SYNC_INTERVAL_MINUTES);
    Ok(clamp_interval(minutes))
}

/// Reads a single account's POP3 server-deletion policy (default `false`).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn pop3_delete_enabled(store: &Store, account_id: i64) -> Result<bool, MailError> {
    Ok(store
        .settings()
        .get(&pop3_delete_key(account_id))
        .await?
        .is_some_and(|value| value == "true"))
}

/// Persists the notifications-enabled flag.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn set_notifications_enabled(store: &Store, enabled: bool) -> Result<(), MailError> {
    store
        .settings()
        .set(KEY_NOTIFICATIONS, bool_str(enabled))
        .await
}

/// Persists the sync interval, clamping it to the one-minute floor first.
/// Returns the value actually stored.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn set_sync_interval_minutes(store: &Store, minutes: u32) -> Result<u32, MailError> {
    let clamped = clamp_interval(minutes);
    store
        .settings()
        .set(KEY_SYNC_INTERVAL, &clamped.to_string())
        .await?;
    Ok(clamped)
}

/// Persists a single account's POP3 server-deletion policy.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn set_pop3_delete_enabled(
    store: &Store,
    account_id: i64,
    enabled: bool,
) -> Result<(), MailError> {
    store
        .settings()
        .set(&pop3_delete_key(account_id), bool_str(enabled))
        .await
}

/// Canonical string form of a boolean setting.
fn bool_str(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}
