//! OpenPGP command surface (Track 5.1): key management, composer sign/encrypt
//! availability, and the reading pane's verify/decrypt analysis.
//!
//! Each `#[tauri::command]` is a thin wrapper over an `*_impl` in a submodule,
//! pulling shared state from the runtime and redacting errors (ADR-0003).

mod manage;
mod read;
mod send;

pub(crate) use send::{PgpSendPlan, resolve_send_plan};

// Exposed for integration tests (mirrors the `*_impl` convention).
pub use manage::{generate_key_impl, import_recipient_impl, list_own_impl};
pub use read::compose_status_impl;

use crate::commands::map_command_error;
use crate::dto::{PgpComposeStatusDto, PgpKeyDto, PgpMessageStatusDto, PgpRecipientKeyDto};
use crate::state::AppState;

/// Normalises an address string to its lowercase email for key lookups.
pub(super) fn normalize_email(address: &str) -> String {
    let inner = address
        .rsplit_once('<')
        .and_then(|(_, rest)| rest.split_once('>').map(|(email, _)| email))
        .unwrap_or(address);
    inner.trim().to_lowercase()
}

/// Generates a new OpenPGP keypair for `user_id`, optionally passphrase-locked.
#[tauri::command]
pub async fn pgp_generate_key(
    state: tauri::State<'_, AppState>,
    user_id: String,
    passphrase: Option<String>,
) -> Result<PgpKeyDto, String> {
    manage::generate_key_impl(state.store(), state.secrets(), user_id, passphrase)
        .await
        .map_err(map_command_error)
}

/// Imports an existing armored secret key as one of our own keys.
#[tauri::command]
pub async fn pgp_import_secret_key(
    state: tauri::State<'_, AppState>,
    armored: String,
) -> Result<PgpKeyDto, String> {
    manage::import_secret_impl(state.store(), state.secrets(), armored)
        .await
        .map_err(map_command_error)
}

/// Imports a recipient's armored public key for an email address.
#[tauri::command]
pub async fn pgp_import_recipient_key(
    state: tauri::State<'_, AppState>,
    email: String,
    armored: String,
) -> Result<PgpRecipientKeyDto, String> {
    manage::import_recipient_impl(state.store(), normalize_email(&email), armored)
        .await
        .map_err(map_command_error)
}

/// Lists our own OpenPGP keys.
#[tauri::command]
pub async fn pgp_list_own_keys(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<PgpKeyDto>, String> {
    manage::list_own_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Lists the recipient public keys we hold.
#[tauri::command]
pub async fn pgp_list_recipient_keys(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<PgpRecipientKeyDto>, String> {
    manage::list_recipients_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Exports the armored public key of one of our keys.
#[tauri::command]
pub async fn pgp_export_public_key(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    manage::export_public_impl(state.store(), id)
        .await
        .map_err(map_command_error)
}

/// Exports the armored secret key of one of our keys (user-initiated backup).
#[tauri::command]
pub async fn pgp_export_secret_key(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    manage::export_secret_impl(state.store(), state.secrets(), id)
        .await
        .map_err(map_command_error)
}

/// Makes one of our keys the default signing key.
#[tauri::command]
pub async fn pgp_set_default_key(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    manage::set_default_impl(state.store(), id)
        .await
        .map_err(map_command_error)
}

/// Deletes one of our keys and purges its keychain secret.
#[tauri::command]
pub async fn pgp_delete_own_key(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<bool, String> {
    manage::delete_own_impl(state.store(), state.secrets(), id)
        .await
        .map_err(map_command_error)
}

/// Deletes a recipient public key.
#[tauri::command]
pub async fn pgp_delete_recipient_key(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<bool, String> {
    manage::delete_recipient_impl(state.store(), id)
        .await
        .map_err(map_command_error)
}

/// Reports whether an outgoing message can be signed and/or encrypted.
#[tauri::command]
pub async fn pgp_compose_status(
    state: tauri::State<'_, AppState>,
    from_email: String,
    recipients: Vec<String>,
) -> Result<PgpComposeStatusDto, String> {
    read::compose_status_impl(state.store(), from_email, recipients)
        .await
        .map_err(map_command_error)
}

/// Analyses a received message's OpenPGP status (verify + decrypt).
#[tauri::command]
pub async fn analyze_message_pgp(
    state: tauri::State<'_, AppState>,
    message_id: i64,
    passphrase: Option<String>,
) -> Result<PgpMessageStatusDto, String> {
    read::analyze_impl(
        state.store(),
        state.factory(),
        state.raw_root(),
        state.secrets(),
        message_id,
        passphrase,
        state.is_offline(),
    )
    .await
    .map_err(map_command_error)
}
