//! The compose command surface: sending, the outbox, and their shared
//! submission pipeline.
//!
//! A send always goes through the outbox: the message is queued, moved into
//! `sending` by a guarded transition (the duplicate-send guard), submitted over
//! SMTP, and then recorded as `sent` or `failed`. Permanent failures are never
//! retried automatically — only the explicit `retry_outbox` command re-runs the
//! flow, and its guarded transition means a row already `sending` or `sent` is
//! never submitted twice.
//!
//! Split across submodules so no file approaches the size gate and the Wave 3
//! compose tracks (rich text, attachments, reply/forward) have headroom:
//!
//! - [`send`] — `send_message` (validate → queue → schedule/offline branch →
//!   submit) and sender-identity resolution.
//! - [`outbox`] — `retry_outbox` and `list_outbox`.
//! - [`shared`] — the SMTP submission pipeline (`submit_record`) plus the small
//!   helpers both entry points reuse (account/address loading, provider and
//!   message builders, local Sent-copy filing).
//!
//! The public `*_impl` functions and their `#[tauri::command]` wrappers are
//! re-exported here so `commands::compose::<name>` keeps resolving for the
//! `generate_handler!` array in `lib.rs` and for the integration tests.

mod outbox;
mod send;
mod shared;

// Glob re-exports (not named) so the hidden helpers the `#[tauri::command]`
// macro generates next to each command (`__cmd__<name>`,
// `__tauri_command_name_<name>`) are reachable at `commands::compose::<name>` —
// `generate_handler!` in `lib.rs` resolves them there. This also re-exports the
// `*_impl` functions the integration tests import from `commands::compose`.
pub use outbox::*;
pub use send::*;
