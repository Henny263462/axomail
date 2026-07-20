//! The `discover_autoconfig` command: look up a provider's server settings for
//! an email address so account setup can pre-fill the form.
//!
//! This is a read-only network probe. It sends **no** credentials — only the
//! email *domain* appears in any lookup URL (ADR-0009 HTTPS client) — and
//! persists nothing. The returned [`AutoconfigResultDto`] is a suggestion the
//! user reviews, tests, and may edit before an account is created.

use crate::autoconfig;
use crate::dto::AutoconfigResultDto;

/// Runs the layered lookup for `email` and maps the outcome to its DTO.
///
/// Infallible by design: transport and parse failures degrade to the next
/// source and, ultimately, to a "not found" result rather than an error.
pub async fn discover_autoconfig_impl(email: &str) -> AutoconfigResultDto {
    match autoconfig::discover(email).await {
        Some(outcome) => AutoconfigResultDto::found(&outcome),
        None => AutoconfigResultDto::not_found(),
    }
}

/// Looks up incoming/outgoing server settings for an email address.
///
/// Never sends the password or local part; only the domain is used to build
/// the HTTPS lookup URLs.
#[tauri::command]
pub async fn discover_autoconfig(email: String) -> Result<AutoconfigResultDto, String> {
    Ok(discover_autoconfig_impl(&email).await)
}
