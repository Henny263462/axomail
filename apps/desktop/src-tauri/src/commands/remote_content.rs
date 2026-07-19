//! The `set_remote_content_allowed` command: manage the per-sender
//! remote-content allow list.
//!
//! "Always allow from this sender" in the reading pane calls this to persist
//! the decision; passing `allowed = false` removes the sender again. The
//! message-detail command consults the same allow list when deciding whether
//! to keep remote images (see [`crate::commands::detail`]).

use axomail_core::MailError;
use axomail_storage::Store;

use crate::commands::map_command_error;
use crate::state::AppState;

/// Adds or removes `sender` from the persistent remote-content allow list.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if `sender` is empty and
/// [`MailError::Database`] on any query failure.
pub async fn set_remote_content_allowed_impl(
    store: &Store,
    sender: &str,
    allowed: bool,
) -> Result<(), MailError> {
    store.remote_content().set_allowed(sender, allowed).await
}

/// Persists (or clears) an "always show remote content" decision for a sender.
#[tauri::command]
pub async fn set_remote_content_allowed(
    state: tauri::State<'_, AppState>,
    sender: String,
    allowed: bool,
) -> Result<(), String> {
    set_remote_content_allowed_impl(state.store(), &sender, allowed)
        .await
        .map_err(map_command_error)
}
