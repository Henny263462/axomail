//! Typed persistence of user settings over the `app_settings` key/value
//! table ([`axomail_storage::SettingsRepo`]).
//!
//! # Stored keys
//!
//! | key | value | default |
//! |-----|-------|---------|
//! | `notifications_enabled` | `"true"`/`"false"` | `true` |
//! | `sync_interval_minutes` | decimal integer, clamped `>= 1` | `10` |
//! | `theme` | `"system"`/`"light"`/`"dark"` | `system` |
//! | `auto_mark_read_enabled` | `"true"`/`"false"` | `true` |
//! | `auto_mark_read_delay_ms` | decimal integer, clamped `<= 30000` | `0` |
//! | `work_offline` | `"true"`/`"false"` | `false` |
//! | `list_threaded` | `"true"`/`"false"` | `false` |
//! | `pop3_delete_policy.<accountId>` | `"true"`/`"false"` | `false` |
//!
//! The POP3 delete policy is the second of the two guards against POP3's
//! destructive `DELE`: it is written **only** by the explicit
//! `set_pop3_delete_policy` command, never by account setup and never by a
//! bulk settings update, so server-side deletion can never be switched on as
//! a side effect.

use axomail_core::MailError;
use axomail_storage::Store;

use crate::dto::ThemePreference;

/// Whether desktop notifications are shown when a sync brings new mail.
const KEY_NOTIFICATIONS: &str = "notifications_enabled";
/// Base background-sync polling interval, in whole minutes.
const KEY_SYNC_INTERVAL: &str = "sync_interval_minutes";
/// Preferred application color theme.
const KEY_THEME: &str = "theme";
/// Whether a message is marked read automatically when it is displayed.
const KEY_AUTO_MARK_READ: &str = "auto_mark_read_enabled";
/// Delay before auto-marking a displayed message read, in milliseconds.
const KEY_AUTO_MARK_READ_DELAY: &str = "auto_mark_read_delay_ms";
/// Whether the app is in Work Offline mode (no network jobs; queue writes).
const KEY_WORK_OFFLINE: &str = "work_offline";
/// Whether the message list groups messages into conversation threads.
const KEY_LIST_THREADED: &str = "list_threaded";
/// Prefix of the per-account POP3 server-deletion policy key.
const KEY_POP3_DELETE_PREFIX: &str = "pop3_delete_policy.";

/// Notifications are on by default; they only ever carry an aggregate count.
const DEFAULT_NOTIFICATIONS: bool = true;
/// Default polling interval when the user has not chosen one.
pub const DEFAULT_SYNC_INTERVAL_MINUTES: u32 = 10;
/// Hard floor on the polling interval; short-interval polling is a non-goal.
pub const MIN_SYNC_INTERVAL_MINUTES: u32 = 1;
/// Auto-mark-read is on by default (mirrors the pre-existing mark-on-open UX).
const DEFAULT_AUTO_MARK_READ: bool = true;
/// Default auto-mark-read delay: immediate (0 ms) on display.
pub const DEFAULT_AUTO_MARK_READ_DELAY_MS: u32 = 0;
/// Ceiling on the auto-mark-read delay (30 s); longer values are pointless.
pub const MAX_AUTO_MARK_READ_DELAY_MS: u32 = 30_000;
/// The message list starts flat (not threaded) so a fresh install behaves like
/// the pre-threading UI until the user opts in.
const DEFAULT_LIST_THREADED: bool = false;

/// Clamps a requested auto-mark-read delay to the supported ceiling.
#[must_use]
pub fn clamp_auto_mark_read_delay(delay_ms: u32) -> u32 {
    delay_ms.min(MAX_AUTO_MARK_READ_DELAY_MS)
}

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

/// Reads the preferred color theme (default [`ThemePreference::System`]).
///
/// An absent or unrecognized stored value resolves to the system default, so
/// a corrupt row can never leave the UI without a theme.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn theme(store: &Store) -> Result<ThemePreference, MailError> {
    Ok(store
        .settings()
        .get(KEY_THEME)
        .await?
        .map_or(ThemePreference::default(), |value| {
            ThemePreference::from_key(&value)
        }))
}

/// Reads whether displayed messages are auto-marked read (default `true`).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn auto_mark_read_enabled(store: &Store) -> Result<bool, MailError> {
    Ok(store
        .settings()
        .get(KEY_AUTO_MARK_READ)
        .await?
        .map_or(DEFAULT_AUTO_MARK_READ, |value| value == "true"))
}

/// Reads the auto-mark-read delay in milliseconds, clamped to the ceiling
/// (default [`DEFAULT_AUTO_MARK_READ_DELAY_MS`]).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn auto_mark_read_delay_ms(store: &Store) -> Result<u32, MailError> {
    let stored = store.settings().get(KEY_AUTO_MARK_READ_DELAY).await?;
    let delay = stored
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_AUTO_MARK_READ_DELAY_MS);
    Ok(clamp_auto_mark_read_delay(delay))
}

/// Persists the auto-mark-read flag and its (clamped) delay together.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn set_auto_mark_read(
    store: &Store,
    enabled: bool,
    delay_ms: u32,
) -> Result<(), MailError> {
    store
        .settings()
        .set(KEY_AUTO_MARK_READ, bool_str(enabled))
        .await?;
    store
        .settings()
        .set(
            KEY_AUTO_MARK_READ_DELAY,
            &clamp_auto_mark_read_delay(delay_ms).to_string(),
        )
        .await
}

/// Reads whether the app is in Work Offline mode (default `false`).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn work_offline(store: &Store) -> Result<bool, MailError> {
    Ok(store
        .settings()
        .get(KEY_WORK_OFFLINE)
        .await?
        .is_some_and(|value| value == "true"))
}

/// Persists the Work Offline mode flag.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn set_work_offline(store: &Store, offline: bool) -> Result<(), MailError> {
    store
        .settings()
        .set(KEY_WORK_OFFLINE, bool_str(offline))
        .await
}

/// Reads whether the message list groups messages into threads (default
/// `false`, i.e. a flat newest-first list).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn list_threaded(store: &Store) -> Result<bool, MailError> {
    Ok(store
        .settings()
        .get(KEY_LIST_THREADED)
        .await?
        .map_or(DEFAULT_LIST_THREADED, |value| value == "true"))
}

/// Persists whether the message list groups messages into threads.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn set_list_threaded(store: &Store, threaded: bool) -> Result<(), MailError> {
    store
        .settings()
        .set(KEY_LIST_THREADED, bool_str(threaded))
        .await
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

/// Persists the preferred color theme.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn set_theme(store: &Store, preference: ThemePreference) -> Result<(), MailError> {
    store.settings().set(KEY_THEME, preference.as_key()).await
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
