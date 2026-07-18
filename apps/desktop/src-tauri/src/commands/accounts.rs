//! Account listing, creation, and deletion commands.

use std::num::NonZeroU16;
use std::sync::Arc;

use axomail_core::{
    Account, AccountId, CredentialRef, EmailAddress, IncomingServerConfig, MailError, Mailbox,
    MailboxName, MailboxRole, OutgoingServerConfig, ServerEndpoint,
};
use axomail_security::{Direction, credential_ref_for_account};
use axomail_storage::{NewAccount, NewMailbox, Store};

use crate::commands::map_command_error;
use crate::commands::messages::count_unread;
use crate::dto::{AccountSummaryDto, CreateAccountInput, MailboxSummaryDto};
use crate::state::{AppState, SharedSecretStore};

/// Default mailbox set created for a new account, in display order, so the UI
/// has a folder tree before any sync exists. IMAP folder discovery in phase 2
/// upserts real folders by name over this baseline.
const DEFAULT_MAILBOXES: [(&str, MailboxRole); 5] = [
    ("Inbox", MailboxRole::Inbox),
    ("Drafts", MailboxRole::Drafts),
    ("Sent", MailboxRole::Sent),
    ("Archive", MailboxRole::Archive),
    ("Trash", MailboxRole::Trash),
];

/// Lists every configured account with its mailbox tree and unread counts.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Configuration`] if a stored id is out of the safe integer
/// range.
pub async fn list_accounts_impl(store: &Store) -> Result<Vec<AccountSummaryDto>, MailError> {
    let accounts = store.accounts().list().await?;
    let mut summaries = Vec::with_capacity(accounts.len());
    for account in &accounts {
        let mailboxes = store.mailboxes().list(account.id).await?;
        let mut mailbox_summaries = Vec::with_capacity(mailboxes.len());
        for mailbox in &mailboxes {
            let unread = count_unread(store, mailbox.id).await?;
            mailbox_summaries.push(MailboxSummaryDto::from_domain(mailbox, unread)?);
        }
        summaries.push(AccountSummaryDto::new(account, mailbox_summaries)?);
    }
    Ok(summaries)
}

/// Creates an account: validates the input, stores both passwords in the
/// keychain, inserts the row referencing them, and seeds the default
/// mailboxes.
///
/// Secrets are written before the row so a keychain failure never leaves an
/// account that cannot authenticate; any failure after a secret is stored
/// rolls the stored secrets (and a partial row) back.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for invalid input, [`MailError::Keychain`]
/// if a secret cannot be stored, and [`MailError::Database`] on any query
/// failure.
pub async fn create_account_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    input: CreateAccountInput,
) -> Result<AccountSummaryDto, MailError> {
    // 1. Validate every field into a core value type before touching secrets.
    let email = EmailAddress::parse(&input.email)?;
    let incoming_endpoint =
        ServerEndpoint::new(input.incoming.host, nonzero_port(input.incoming.port)?)?;
    let outgoing_endpoint =
        ServerEndpoint::new(input.outgoing.host, nonzero_port(input.outgoing.port)?)?;

    // 2. Derive credential references from the id the row will receive. The
    //    prediction is verified against the assigned id in `finish_account`.
    let account_id = AccountId::new(next_account_id(store).await?);
    let incoming_ref = credential_ref_for_account(account_id, Direction::Incoming);
    let outgoing_ref = credential_ref_for_account(account_id, Direction::Outgoing);

    // 3. Store both secrets first.
    set_secret(secrets, incoming_ref.clone(), input.incoming.password).await?;
    if let Err(error) = set_secret(secrets, outgoing_ref.clone(), input.outgoing.password).await {
        delete_secret_best_effort(secrets, &incoming_ref).await;
        return Err(error);
    }

    // 4. Insert the account row referencing the stored secrets.
    let new_account = NewAccount {
        display_name: input.display_name,
        email,
        incoming: IncomingServerConfig {
            protocol: input.incoming.protocol,
            endpoint: incoming_endpoint,
            security: input.incoming.security,
            username: input.incoming.username,
            credential: incoming_ref.clone(),
        },
        outgoing: OutgoingServerConfig {
            endpoint: outgoing_endpoint,
            security: input.outgoing.security,
            username: input.outgoing.username,
            credential: outgoing_ref.clone(),
        },
    };
    let account = match store.accounts().insert(&new_account).await {
        Ok(account) => account,
        Err(error) => {
            delete_secret_best_effort(secrets, &incoming_ref).await;
            delete_secret_best_effort(secrets, &outgoing_ref).await;
            return Err(error);
        }
    };

    // 5. Seed mailboxes and build the DTO; roll everything back on failure.
    match finish_account(store, &account, account_id).await {
        Ok(summary) => Ok(summary),
        Err(error) => {
            let _ = store.accounts().delete(account.id).await;
            delete_secret_best_effort(secrets, &incoming_ref).await;
            delete_secret_best_effort(secrets, &outgoing_ref).await;
            Err(error)
        }
    }
}

/// Deletes an account and its keychain secrets.
///
/// The row deletion cascades to mailboxes and messages. Secret deletion is
/// best-effort and idempotent: it runs even when no row existed and never
/// fails the command, so a missing secret cannot block account removal.
///
/// # Errors
///
/// Returns [`MailError::Database`] if the row deletion query fails.
pub async fn delete_account_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    account_id: i64,
) -> Result<(), MailError> {
    let id = AccountId::new(account_id);
    store.accounts().delete(id).await?;
    for direction in [Direction::Incoming, Direction::Outgoing] {
        let reference = credential_ref_for_account(id, direction);
        delete_secret_best_effort(secrets, &reference).await;
    }
    Ok(())
}

/// Verifies the assigned id, seeds mailboxes, and assembles the account DTO.
async fn finish_account(
    store: &Store,
    account: &Account,
    expected_id: AccountId,
) -> Result<AccountSummaryDto, MailError> {
    if account.id != expected_id {
        return Err(MailError::Database {
            message: "create_account: account id allocation was not sequential".to_owned(),
        });
    }
    let mailboxes = create_default_mailboxes(store, account.id).await?;
    let summaries = mailboxes
        .iter()
        .map(|mailbox| MailboxSummaryDto::from_domain(mailbox, 0))
        .collect::<Result<Vec<_>, _>>()?;
    AccountSummaryDto::new(account, summaries)
}

/// Predicts the id the next inserted account row will receive.
async fn next_account_id(store: &Store) -> Result<i64, MailError> {
    let accounts = store.accounts().list().await?;
    let max = accounts
        .iter()
        .map(|account| account.id.value())
        .max()
        .unwrap_or(0);
    Ok(max + 1)
}

/// Converts a port number into a [`NonZeroU16`], rejecting zero.
fn nonzero_port(port: u16) -> Result<NonZeroU16, MailError> {
    NonZeroU16::new(port).ok_or_else(|| MailError::Configuration {
        message: "server port must not be zero".to_owned(),
    })
}

/// Creates the default mailbox set for a freshly inserted account.
async fn create_default_mailboxes(
    store: &Store,
    account_id: AccountId,
) -> Result<Vec<Mailbox>, MailError> {
    let mut mailboxes = Vec::with_capacity(DEFAULT_MAILBOXES.len());
    for (name, role) in DEFAULT_MAILBOXES {
        let mailbox = store
            .mailboxes()
            .upsert(&NewMailbox {
                account: account_id,
                name: MailboxName::new(name),
                role,
                uid_validity: None,
                uid_next: None,
            })
            .await?;
        mailboxes.push(mailbox);
    }
    Ok(mailboxes)
}

/// Stores a secret on a blocking task (keychain I/O blocks).
async fn set_secret(
    secrets: &SharedSecretStore,
    reference: CredentialRef,
    secret: String,
) -> Result<(), MailError> {
    let secrets = Arc::clone(secrets);
    match tokio::task::spawn_blocking(move || secrets.set_secret(&reference, &secret)).await {
        Ok(result) => result,
        Err(_) => Err(MailError::Keychain {
            message: "keychain write did not complete".to_owned(),
        }),
    }
}

/// Deletes a secret on a blocking task, logging but swallowing any failure.
async fn delete_secret_best_effort(secrets: &SharedSecretStore, reference: &CredentialRef) {
    let secrets = Arc::clone(secrets);
    let reference = reference.clone();
    match tokio::task::spawn_blocking(move || secrets.delete_secret(&reference)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(error = %error, "failed to delete account secret from keychain");
        }
        Err(_) => tracing::warn!("keychain deletion task did not complete"),
    }
}

/// Lists every configured account.
#[tauri::command]
pub async fn list_accounts(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AccountSummaryDto>, String> {
    list_accounts_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Creates a new account from the setup form input.
#[tauri::command]
pub async fn create_account(
    state: tauri::State<'_, AppState>,
    input: CreateAccountInput,
) -> Result<AccountSummaryDto, String> {
    let summary = create_account_impl(state.store(), state.secrets(), input)
        .await
        .map_err(map_command_error)?;
    // The new account's inbox joins the background-sync target set.
    refresh_scheduler_targets(&state).await;
    Ok(summary)
}

/// Deletes an account and its keychain secrets.
#[tauri::command]
pub async fn delete_account(
    state: tauri::State<'_, AppState>,
    account_id: i64,
) -> Result<(), String> {
    delete_account_impl(state.store(), state.secrets(), account_id)
        .await
        .map_err(map_command_error)?;
    // Drop the removed account's inbox from the background-sync target set.
    refresh_scheduler_targets(&state).await;
    Ok(())
}

/// Pushes the current inbox target set into the scheduler, logging (never
/// surfacing) a failure: the account change already succeeded.
async fn refresh_scheduler_targets(state: &AppState) {
    if let Err(error) = state.scheduler().refresh_targets().await {
        tracing::warn!(error = %error, "could not refresh background-sync targets");
    }
}
