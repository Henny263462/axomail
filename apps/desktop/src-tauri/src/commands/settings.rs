//! The settings commands: `get_settings`, `update_settings`, and the guarded
//! `set_pop3_delete_policy`.
//!
//! `update_settings` persists the notification flag and the sync interval and
//! pushes the (clamped) interval and current targets into the background
//! scheduler. It deliberately does **not** apply the DTO's
//! `pop3DeletePolicies`: enabling POP3 server deletion is possible only
//! through [`set_pop3_delete_policy`], the second of the adapter's two delete
//! guards.

use axomail_core::{AccountId, IncomingProtocol, MailError};
use axomail_storage::Store;

use crate::commands::map_command_error;
use crate::dto::{Pop3DeletePolicyDto, SettingsDto};
use crate::scheduler::SchedulerControl;
use crate::settings;
use crate::state::AppState;

/// Reads the current settings, including each POP3 account's delete policy.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Configuration`] if a stored id is out of the safe range.
pub async fn get_settings_impl(store: &Store) -> Result<SettingsDto, MailError> {
    let notifications_enabled = settings::notifications_enabled(store).await?;
    let sync_interval_minutes = settings::sync_interval_minutes(store).await?;

    let mut pop3_delete_policies = Vec::new();
    for account in store.accounts().list().await? {
        if account.incoming.protocol == IncomingProtocol::Pop3 {
            let enabled = settings::pop3_delete_enabled(store, account.id.value()).await?;
            pop3_delete_policies.push(Pop3DeletePolicyDto::new(account.id.value(), enabled)?);
        }
    }

    Ok(SettingsDto {
        notifications_enabled,
        sync_interval_minutes,
        pop3_delete_policies,
    })
}

/// Persists the notification flag and interval, then pushes the clamped
/// interval and current targets into the scheduler.
///
/// The DTO's `pop3DeletePolicies` are ignored on purpose; see the module
/// docs and [`set_pop3_delete_policy_impl`].
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn update_settings_impl(
    store: &Store,
    scheduler: &SchedulerControl,
    input: SettingsDto,
) -> Result<(), MailError> {
    settings::set_notifications_enabled(store, input.notifications_enabled).await?;
    let interval = settings::set_sync_interval_minutes(store, input.sync_interval_minutes).await?;
    // Relaunch the scheduler so the new interval takes effect (its interval is
    // fixed at construction) and the target set is refreshed.
    scheduler.launch(interval).await?;
    Ok(())
}

/// Sets one account's POP3 server-deletion policy after verifying the account
/// exists and is a POP3 account. This explicit call is the only way to enable
/// server deletion.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the account does not exist and
/// [`MailError::UnsupportedCapability`] if it is not a POP3 account.
pub async fn set_pop3_delete_policy_impl(
    store: &Store,
    account_id: i64,
    enabled: bool,
) -> Result<(), MailError> {
    let account = store
        .accounts()
        .get(AccountId::new(account_id))
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "cannot set a delete policy for an account that does not exist".to_owned(),
        })?;
    if account.incoming.protocol != IncomingProtocol::Pop3 {
        return Err(MailError::UnsupportedCapability {
            capability: "server-side deletion policy (only POP3 accounts have one)".to_owned(),
        });
    }
    settings::set_pop3_delete_enabled(store, account_id, enabled).await
}

/// Reads the current application settings.
#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, AppState>) -> Result<SettingsDto, String> {
    get_settings_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Updates the notification flag and sync interval.
#[tauri::command]
pub async fn update_settings(
    state: tauri::State<'_, AppState>,
    settings: SettingsDto,
) -> Result<(), String> {
    update_settings_impl(state.store(), state.scheduler(), settings)
        .await
        .map_err(map_command_error)
}

/// Sets an account's POP3 server-deletion policy (the second delete guard).
#[tauri::command]
pub async fn set_pop3_delete_policy(
    state: tauri::State<'_, AppState>,
    account_id: i64,
    enabled: bool,
) -> Result<(), String> {
    set_pop3_delete_policy_impl(state.store(), account_id, enabled)
        .await
        .map_err(map_command_error)
}
