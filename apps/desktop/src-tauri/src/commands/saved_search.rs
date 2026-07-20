//! Advanced-search and saved-search commands.
//!
//! `run_advanced_search` runs an ad-hoc query built in the advanced-search
//! dialog; the saved-search commands persist a named query and run it later.
//! Both paths funnel matched messages through [`to_results`], which joins each
//! hit with its stored envelope (mirroring `search_messages`) so the results
//! view shows subject, sender, and date without a second round-trip.

use axomail_core::MailError;
use axomail_storage::{
    SavedSearchId, SearchHit, SearchMatch, Store, validate_saved_search_input,
    validate_search_query,
};

use crate::commands::map_command_error;
use crate::dto::{SavedSearchDto, SavedSearchInputDto, SearchQueryDto, SearchResultDto};
use crate::state::AppState;

/// Default cap on the number of matched messages a run returns.
const RESULT_LIMIT: u32 = 200;

/// Joins each matched message with its envelope to build a result DTO. A hit
/// whose message has vanished since it matched is skipped, never failing the run.
async fn to_results(
    store: &Store,
    matches: Vec<SearchMatch>,
) -> Result<Vec<SearchResultDto>, MailError> {
    let mut results = Vec::with_capacity(matches.len());
    for hit in &matches {
        let Some(envelope) = store.messages().get(hit.message_id).await? else {
            continue;
        };
        let search_hit = SearchHit {
            message_id: hit.message_id,
            mailbox_id: hit.mailbox_id,
            snippet: hit.snippet.clone(),
        };
        results.push(SearchResultDto::from_parts(
            &search_hit,
            &envelope,
            hit.account.value(),
        )?);
    }
    Ok(results)
}

/// Runs an ad-hoc advanced-search query and returns the matched messages.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an invalid query (a bad regex or a
/// folder scope with no folder) and [`MailError::Database`] on any query failure.
pub async fn run_advanced_search_impl(
    store: &Store,
    query: SearchQueryDto,
) -> Result<Vec<SearchResultDto>, MailError> {
    let query = query.into_query()?;
    validate_search_query(&query)?;
    let matches = store.saved_searches().run(&query, RESULT_LIMIT).await?;
    to_results(store, matches).await
}

/// Lists every saved search, alphabetically.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure or a bad decode.
pub async fn list_saved_searches_impl(store: &Store) -> Result<Vec<SavedSearchDto>, MailError> {
    store
        .saved_searches()
        .list()
        .await?
        .iter()
        .map(SavedSearchDto::from_saved)
        .collect()
}

/// Creates a saved search and returns it.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for invalid content and
/// [`MailError::Database`] on any query failure.
pub async fn create_saved_search_impl(
    store: &Store,
    input: SavedSearchInputDto,
) -> Result<SavedSearchDto, MailError> {
    let input = input.into_input()?;
    validate_saved_search_input(&input)?;
    let saved = store.saved_searches().create(&input).await?;
    SavedSearchDto::from_saved(&saved)
}

/// Replaces a saved search's name and query and returns it.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for invalid content or an unknown saved
/// search, and [`MailError::Database`] on any query failure.
pub async fn update_saved_search_impl(
    store: &Store,
    saved_search_id: i64,
    input: SavedSearchInputDto,
) -> Result<SavedSearchDto, MailError> {
    let input = input.into_input()?;
    validate_saved_search_input(&input)?;
    let saved = store
        .saved_searches()
        .update(SavedSearchId::new(saved_search_id), &input)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "cannot update a saved search that does not exist".to_owned(),
        })?;
    SavedSearchDto::from_saved(&saved)
}

/// Deletes a saved search. Deleting an absent one is a silent no-op.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn delete_saved_search_impl(
    store: &Store,
    saved_search_id: i64,
) -> Result<(), MailError> {
    store
        .saved_searches()
        .delete(SavedSearchId::new(saved_search_id))
        .await?;
    Ok(())
}

/// Runs a saved search by id and returns the matched messages. A saved search
/// that was deleted since it was listed resolves to an empty result rather than
/// an error (a benign list-then-click race).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure or a bad decode.
pub async fn run_saved_search_impl(
    store: &Store,
    saved_search_id: i64,
) -> Result<Vec<SearchResultDto>, MailError> {
    let matches = store
        .saved_searches()
        .run_saved(SavedSearchId::new(saved_search_id), RESULT_LIMIT)
        .await?
        .unwrap_or_default();
    to_results(store, matches).await
}

/// Runs an ad-hoc advanced-search query.
#[tauri::command]
pub async fn run_advanced_search(
    state: tauri::State<'_, AppState>,
    query: SearchQueryDto,
) -> Result<Vec<SearchResultDto>, String> {
    run_advanced_search_impl(state.store(), query)
        .await
        .map_err(map_command_error)
}

/// Lists the saved searches.
#[tauri::command]
pub async fn list_saved_searches(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<SavedSearchDto>, String> {
    list_saved_searches_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Creates a saved search.
#[tauri::command]
pub async fn create_saved_search(
    state: tauri::State<'_, AppState>,
    input: SavedSearchInputDto,
) -> Result<SavedSearchDto, String> {
    create_saved_search_impl(state.store(), input)
        .await
        .map_err(map_command_error)
}

/// Updates a saved search.
#[tauri::command]
pub async fn update_saved_search(
    state: tauri::State<'_, AppState>,
    saved_search_id: i64,
    input: SavedSearchInputDto,
) -> Result<SavedSearchDto, String> {
    update_saved_search_impl(state.store(), saved_search_id, input)
        .await
        .map_err(map_command_error)
}

/// Deletes a saved search.
#[tauri::command]
pub async fn delete_saved_search(
    state: tauri::State<'_, AppState>,
    saved_search_id: i64,
) -> Result<(), String> {
    delete_saved_search_impl(state.store(), saved_search_id)
        .await
        .map_err(map_command_error)
}

/// Runs a saved search.
#[tauri::command]
pub async fn run_saved_search(
    state: tauri::State<'_, AppState>,
    saved_search_id: i64,
) -> Result<Vec<SearchResultDto>, String> {
    run_saved_search_impl(state.store(), saved_search_id)
        .await
        .map_err(map_command_error)
}
