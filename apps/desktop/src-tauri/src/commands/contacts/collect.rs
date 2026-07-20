//! Auto-collection of sent recipients into the address book, plus the setting
//! that toggles it.
//!
//! On a successful send the compose path calls [`collect_sent_recipients`],
//! which — when the `collect_addresses_on_send` setting is on (the default) —
//! upserts each To/Cc/Bcc address into the global address book. Collection is
//! best-effort: it never fails or delays the send, and a name-only contact is
//! created only for an address not already known (see
//! `ContactRepo::collect_address`). Scheduled and offline sends drain later
//! through the sync worker, which does not run collection — a documented
//! limitation of hooking the interactive send path only.

use axomail_core::{EmailAddress, MailError};
use axomail_storage::Store;

use crate::commands::map_command_error;
use crate::state::AppState;

/// Settings key: whether sent recipients are auto-collected into contacts.
const KEY_COLLECT: &str = "collect_addresses_on_send";
/// Auto-collection is on by default (matches Thunderbird's Collected Addresses).
const DEFAULT_COLLECT: bool = true;

/// Reads whether sent recipients are auto-collected (default `true`).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn collect_addresses_enabled(store: &Store) -> Result<bool, MailError> {
    Ok(store
        .settings()
        .get(KEY_COLLECT)
        .await?
        .map_or(DEFAULT_COLLECT, |value| value == "true"))
}

/// Persists the auto-collection flag.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn set_collect_addresses_enabled(store: &Store, enabled: bool) -> Result<(), MailError> {
    store
        .settings()
        .set(KEY_COLLECT, if enabled { "true" } else { "false" })
        .await
}

/// Auto-collects the recipients of a just-sent message. Best-effort: honors the
/// toggle, skips already-known and unparseable addresses, and never propagates
/// an error (a collection failure must not affect the send outcome).
pub async fn collect_sent_recipients(store: &Store, recipients: &[String]) {
    match collect_addresses_enabled(store).await {
        Ok(true) => {}
        Ok(false) => return,
        Err(error) => {
            tracing::warn!(error = %error, "could not read the address-collection setting");
            return;
        }
    }
    for raw in recipients {
        let Ok(email) = EmailAddress::parse(raw.trim()) else {
            continue; // Already validated on the send path; skip defensively.
        };
        if let Err(error) = store.contacts().collect_address(&email, "").await {
            tracing::warn!(error = %error, "could not auto-collect a sent recipient");
        }
    }
}

/// Reads the auto-collection setting.
#[tauri::command]
pub async fn get_collect_addresses(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    collect_addresses_enabled(state.store())
        .await
        .map_err(map_command_error)
}

/// Sets the auto-collection setting.
#[tauri::command]
pub async fn set_collect_addresses(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    set_collect_addresses_enabled(state.store(), enabled)
        .await
        .map_err(map_command_error)
}
