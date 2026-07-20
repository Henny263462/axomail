//! Message-filter commands: CRUD over the rules, reordering, the per-account
//! run-on-arrival toggle, and a manual "run filters on this folder" action.
//!
//! Filter actions execute through [`axomail_sync::run_filters_on_messages`],
//! which reuses the shared write-back primitives; the manual command triggers an
//! `OpReplay` per affected account so any queued server ops are pushed.

use axomail_core::{
    AccountId, EmailAddress, IncomingMailProvider, MailError, MailboxId, MailboxRole,
};
use axomail_storage::{ActionKind, FilterId, FilterInput, Store, validate_filter_input};
use axomail_sync::{ProviderFactory, SyncEngine, WorkRequest, run_filters_on_messages};

use crate::commands::map_command_error;
use crate::dto::{AccountFilterSettingsDto, FilterDto, FilterInputDto, FilterRunOutcomeDto};
use crate::state::AppState;

/// A user-facing configuration error.
fn config(message: &str) -> MailError {
    MailError::Configuration {
        message: message.to_owned(),
    }
}

/// Validates a filter's content (storage-level checks plus action-param sanity)
/// before it is persisted.
fn validate(input: &FilterInput) -> Result<(), MailError> {
    validate_filter_input(input)?;
    for action in &input.actions {
        match action.kind {
            ActionKind::Forward if EmailAddress::parse(action.param.trim()).is_err() => {
                return Err(config("a forward action needs a valid email address"));
            }
            ActionKind::Move | ActionKind::Copy if action.param.trim().parse::<i64>().is_err() => {
                return Err(config("a move or copy action needs a target folder"));
            }
            ActionKind::Tag if action.param.trim().is_empty() => {
                return Err(config("a tag action needs a tag name"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Lists the filters in a scope (an account, or the global rules when
/// `account_id` is `None`), in rule-list order.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn list_filters_impl(
    store: &Store,
    account_id: Option<i64>,
) -> Result<Vec<FilterDto>, MailError> {
    store
        .filters()
        .list(account_id.map(AccountId::new))
        .await?
        .iter()
        .map(FilterDto::from_filter)
        .collect()
}

/// Creates a filter and returns it.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for invalid content and
/// [`MailError::Database`] on any query failure.
pub async fn create_filter_impl(
    store: &Store,
    input: FilterInputDto,
) -> Result<FilterDto, MailError> {
    let input = input.into_input();
    validate(&input)?;
    let filter = store.filters().create(&input).await?;
    FilterDto::from_filter(&filter)
}

/// Replaces a filter's content and returns the updated filter.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for invalid content or an unknown
/// filter, and [`MailError::Database`] on any query failure.
pub async fn update_filter_impl(
    store: &Store,
    filter_id: i64,
    input: FilterInputDto,
) -> Result<FilterDto, MailError> {
    let input = input.into_input();
    validate(&input)?;
    let filter = store
        .filters()
        .update(FilterId::new(filter_id), &input)
        .await?
        .ok_or_else(|| config("cannot update a filter that does not exist"))?;
    FilterDto::from_filter(&filter)
}

/// Deletes a filter. Deleting an absent filter is a silent no-op.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn delete_filter_impl(store: &Store, filter_id: i64) -> Result<(), MailError> {
    store.filters().delete(FilterId::new(filter_id)).await?;
    Ok(())
}

/// Enables or disables a single filter.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn set_filter_enabled_impl(
    store: &Store,
    filter_id: i64,
    enabled: bool,
) -> Result<(), MailError> {
    store
        .filters()
        .set_enabled(FilterId::new(filter_id), enabled)
        .await?;
    Ok(())
}

/// Reorders a scope's filters to match `ordered_ids`.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn reorder_filters_impl(
    store: &Store,
    account_id: Option<i64>,
    ordered_ids: Vec<i64>,
) -> Result<(), MailError> {
    let ids: Vec<FilterId> = ordered_ids.into_iter().map(FilterId::new).collect();
    store
        .filters()
        .reorder(account_id.map(AccountId::new), &ids)
        .await?;
    Ok(())
}

/// Runs the account's filters over every message currently in `mailbox_id`,
/// then triggers an `OpReplay` per affected account.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown folder and
/// [`MailError::Database`] on any query failure.
pub async fn run_filters_on_folder_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    mailbox_id: i64,
) -> Result<FilterRunOutcomeDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mailbox = store
        .mailboxes()
        .get(MailboxId::new(mailbox_id))
        .await?
        .ok_or_else(|| config("the folder does not exist"))?;
    let ids = store.messages().list_ids_in_mailbox(mailbox.id).await?;
    let report = run_filters_on_messages(store, mailbox.account, &ids).await?;
    for account in &report.replay_accounts {
        engine
            .enqueue_work(WorkRequest::OpReplay {
                account_id: *account,
            })
            .await?;
    }
    Ok(FilterRunOutcomeDto::from(report))
}

/// Reads an account's run-on-arrival filtering setting.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn get_account_filter_settings_impl(
    store: &Store,
    account_id: i64,
) -> Result<AccountFilterSettingsDto, MailError> {
    let arrival_enabled = store
        .filters()
        .arrival_enabled(AccountId::new(account_id))
        .await?;
    Ok(AccountFilterSettingsDto { arrival_enabled })
}

/// Sets an account's run-on-arrival filtering toggle. Enabling baselines the
/// high-water mark to the current inbox frontier so it never sweeps existing
/// mail retroactively.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn set_account_filter_arrival_impl(
    store: &Store,
    account_id: i64,
    enabled: bool,
) -> Result<(), MailError> {
    let account = AccountId::new(account_id);
    let frontier = inbox_frontier(store, account).await?;
    store
        .filters()
        .set_arrival_enabled(account, enabled, frontier)
        .await?;
    Ok(())
}

/// The maximum message id in the account's inbox (0 when it has none), used as
/// the arrival baseline.
async fn inbox_frontier(store: &Store, account: AccountId) -> Result<i64, MailError> {
    let inbox = store
        .mailboxes()
        .list(account)
        .await?
        .into_iter()
        .find(|mailbox| mailbox.role == MailboxRole::Inbox)
        .map(|mailbox| mailbox.id);
    match inbox {
        Some(id) => store.filters().max_message_id_in_mailbox(id).await,
        None => Ok(0),
    }
}

/// Lists the filters in a scope.
#[tauri::command]
pub async fn list_filters(
    state: tauri::State<'_, AppState>,
    account_id: Option<i64>,
) -> Result<Vec<FilterDto>, String> {
    list_filters_impl(state.store(), account_id)
        .await
        .map_err(map_command_error)
}

/// Creates a filter.
#[tauri::command]
pub async fn create_filter(
    state: tauri::State<'_, AppState>,
    input: FilterInputDto,
) -> Result<FilterDto, String> {
    create_filter_impl(state.store(), input)
        .await
        .map_err(map_command_error)
}

/// Updates a filter.
#[tauri::command]
pub async fn update_filter(
    state: tauri::State<'_, AppState>,
    filter_id: i64,
    input: FilterInputDto,
) -> Result<FilterDto, String> {
    update_filter_impl(state.store(), filter_id, input)
        .await
        .map_err(map_command_error)
}

/// Deletes a filter.
#[tauri::command]
pub async fn delete_filter(
    state: tauri::State<'_, AppState>,
    filter_id: i64,
) -> Result<(), String> {
    delete_filter_impl(state.store(), filter_id)
        .await
        .map_err(map_command_error)
}

/// Enables or disables a filter.
#[tauri::command]
pub async fn set_filter_enabled(
    state: tauri::State<'_, AppState>,
    filter_id: i64,
    enabled: bool,
) -> Result<(), String> {
    set_filter_enabled_impl(state.store(), filter_id, enabled)
        .await
        .map_err(map_command_error)
}

/// Reorders a scope's filters.
#[tauri::command]
pub async fn reorder_filters(
    state: tauri::State<'_, AppState>,
    account_id: Option<i64>,
    ordered_ids: Vec<i64>,
) -> Result<(), String> {
    reorder_filters_impl(state.store(), account_id, ordered_ids)
        .await
        .map_err(map_command_error)
}

/// Runs filters over a folder now.
#[tauri::command]
pub async fn run_filters_on_folder(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
) -> Result<FilterRunOutcomeDto, String> {
    run_filters_on_folder_impl(state.store(), state.engine(), mailbox_id)
        .await
        .map_err(map_command_error)
}

/// Reads an account's arrival-filtering setting.
#[tauri::command]
pub async fn get_account_filter_settings(
    state: tauri::State<'_, AppState>,
    account_id: i64,
) -> Result<AccountFilterSettingsDto, String> {
    get_account_filter_settings_impl(state.store(), account_id)
        .await
        .map_err(map_command_error)
}

/// Sets an account's arrival-filtering toggle.
#[tauri::command]
pub async fn set_account_filter_arrival(
    state: tauri::State<'_, AppState>,
    account_id: i64,
    enabled: bool,
) -> Result<(), String> {
    set_account_filter_arrival_impl(state.store(), account_id, enabled)
        .await
        .map_err(map_command_error)
}
