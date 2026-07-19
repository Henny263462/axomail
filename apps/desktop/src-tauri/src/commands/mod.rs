//! Tauri command surface.
//!
//! Each submodule exposes a pair of layers: a plain, runtime-independent
//! `*_impl` function that carries the logic and is unit-tested directly, and
//! a thin `#[tauri::command]` wrapper that pulls shared state from the Tauri
//! runtime and maps errors to user-facing strings.

pub mod accounts;
pub mod compose;
pub mod connection;
pub mod detail;
pub mod drafts;
pub mod messages;
pub mod meta;
pub mod remote_content;
pub mod search;
pub mod settings;
pub mod sync;
pub mod unified;

use axomail_core::MailError;

/// Converts a [`MailError`] into the user-facing string returned to the
/// frontend.
///
/// The `Display` form is deliberately used: it is secret-free by
/// construction (ADR-0003), whereas the `Debug` form is only ever written to
/// the diagnostics log, never returned across the IPC boundary.
pub(crate) fn map_command_error(error: MailError) -> String {
    tracing::debug!(error = ?error, "command failed");
    error.to_string()
}
