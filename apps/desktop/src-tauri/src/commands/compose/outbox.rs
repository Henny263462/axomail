//! The outbox commands: `retry_outbox` (re-run a queued/failed send) and
//! `list_outbox` (enumerate every outbox row).

use axomail_core::MailError;
use axomail_storage::{OutboxId, OutboxState, Store};

use super::shared::{SendExtras, load_account, submit_record};
use crate::commands::map_command_error;
use crate::dto::{OutboxItemDto, SendOutcomeDto};
use crate::state::{AppState, SharedSecretStore};

/// Retries a previously queued or failed outbox row.
///
/// The guarded transition is the duplicate-send protection: a row already
/// `sending` or `sent` is not submitted again, and the current state is
/// reported back instead.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the outbox row or its account no
/// longer exists.
pub async fn retry_outbox_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
    outbox_id: i64,
) -> Result<SendOutcomeDto, MailError> {
    let id = OutboxId::new(outbox_id);
    let record = store
        .outbox()
        .get(id)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "no such outbox item".to_owned(),
        })?;

    if !store.outbox().begin_send(id).await? {
        // Already sending or sent: never submit a second time.
        let (sent, error) = match record.state {
            OutboxState::Sent => (true, None),
            _ => (
                false,
                Some("a send for this message is already in progress".to_owned()),
            ),
        };
        return SendOutcomeDto::new(outbox_id, sent, error);
    }

    let account = load_account(store, record.account.value()).await?;
    // A retry/drain re-sends what the outbox row holds; reply threading headers
    // and forwarded attachments are not persisted there, so none are re-applied.
    submit_record(
        store,
        secrets,
        extra_roots,
        &account,
        &record,
        &SendExtras::default(),
    )
    .await
}

/// Lists every outbox row, most recently updated first.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Configuration`] if a stored id is out of the safe range.
pub async fn list_outbox_impl(store: &Store) -> Result<Vec<OutboxItemDto>, MailError> {
    store
        .outbox()
        .list()
        .await?
        .into_iter()
        .map(OutboxItemDto::from_record)
        .collect()
}

/// Retries a failed or queued outbox item.
#[tauri::command]
pub async fn retry_outbox(
    state: tauri::State<'_, AppState>,
    outbox_id: i64,
) -> Result<SendOutcomeDto, String> {
    retry_outbox_impl(state.store(), state.secrets(), &[], outbox_id)
        .await
        .map_err(map_command_error)
}

/// Lists every outbox item.
#[tauri::command]
pub async fn list_outbox(state: tauri::State<'_, AppState>) -> Result<Vec<OutboxItemDto>, String> {
    list_outbox_impl(state.store())
        .await
        .map_err(map_command_error)
}
