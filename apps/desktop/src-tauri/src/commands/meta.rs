//! Metadata commands that need neither storage nor secrets.

/// Returns the application version from the Cargo manifest.
#[tauri::command]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
