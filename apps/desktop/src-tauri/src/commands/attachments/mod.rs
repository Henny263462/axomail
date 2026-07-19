//! Attachment commands, split by direction:
//!
//! - [`compose`] — files the user picks to attach to an outgoing message: the
//!   dialog returns paths, Rust reads the bytes at send time (Track 3.5).
//! - [`save`] — attachments and raw source saved *out* of a received message
//!   (save attachment / save-all / save-as-`.eml` / view-source / all-headers,
//!   Track 3.6).
//!
//! Both follow the ADR-0010 rule: the dialog picks the path, Rust does the IO;
//! the webview never gains a filesystem scope. The public items are re-exported
//! here so every command resolves as `commands::attachments::<name>` (the paths
//! `lib.rs` registers and the integration tests import).

pub mod compose;
pub mod save;

// Glob re-exports (not named) so the hidden helpers the `#[tauri::command]`
// macro generates next to each command (`__cmd__<name>`,
// `__tauri_command_name_<name>`) are reachable at `commands::attachments::<name>`
// — `generate_handler!` in `lib.rs` resolves them there. This also re-exports
// the `*_impl` functions the integration tests import. `load_attachments` is
// `pub(crate)` (used by the send path), so it needs its own crate-visible line.
pub(crate) use compose::load_attachments;
pub use compose::*;
pub use save::*;
