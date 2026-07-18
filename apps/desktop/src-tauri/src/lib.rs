//! axomail desktop application shell.
//!
//! Hosts the Tauri runtime and exposes the command surface to the Vue
//! frontend. All mail logic lives in the `crates/` workspace members; this
//! crate stays a thin composition layer.

/// Returns the application version from the Cargo manifest.
#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Builds and runs the Tauri application.
///
/// Exits the process with a non-zero code if the runtime fails to start;
/// there is no meaningful way to continue without a window.
pub fn run() {
    let outcome = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_version])
        .run(tauri::generate_context!());

    if let Err(error) = outcome {
        eprintln!("fatal: failed to start axomail: {error}");
        std::process::exit(1);
    }
}
