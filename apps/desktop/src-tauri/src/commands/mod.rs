//! Tauri command surface.
//!
//! Each submodule exposes a pair of layers: a plain, runtime-independent
//! `*_impl` function that carries the logic and is unit-tested directly, and
//! a thin `#[tauri::command]` wrapper that pulls shared state from the Tauri
//! runtime and maps errors to user-facing strings.

pub mod account_secrets;
pub mod accounts;
pub mod actions;
pub mod attachments;
pub mod compose;
pub mod connection;
pub mod contacts;
pub mod detail;
pub mod drafts;
pub mod filters;
pub mod folders;
pub mod identities;
pub mod invitations;
pub mod jobs;
pub mod junk;
pub mod mbox;
pub mod messages;
pub mod meta;
pub mod oauth;
pub mod offline;
pub mod outbox_schedule;
pub mod pgp;
pub mod remote_content;
pub mod reply;
pub mod search;
pub mod settings;
pub mod sync;
pub mod tag_seed;
pub mod tags;
pub mod trash;
pub mod unified;
pub mod writeback;

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
