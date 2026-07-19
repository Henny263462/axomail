//! Account listing, creation, and deletion commands.

use std::num::NonZeroU16;
use std::path::Path;

use axomail_core::{
    Account, AccountAuth, AccountId, EmailAddress, IncomingServerConfig, MailError, Mailbox,
    MailboxName, MailboxRole, OutgoingServerConfig, ServerEndpoint,
};
use axomail_security::{Direction, credential_ref_for_account, credential_ref_for_oauth};
use axomail_storage::{
    AccountUpdate, ArchiveGranularity, ForwardMode, IdentityFields, NewAccount, NewIdentity,
    NewMailbox, ReplyPosition, SignaturePlacement, Store,
};

use crate::commands::account_secrets::{
    delete_secret_best_effort, remove_raw_best_effort, set_secret,
};
use crate::commands::map_command_error;
use crate::commands::messages::count_unread;
use crate::dto::{
    AccountDetailDto, AccountSummaryDto, CreateAccountInput, MailboxSummaryDto, UpdateAccountInput,
};
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
    mut input: CreateAccountInput,
) -> Result<AccountSummaryDto, MailError> {
    // 1. Validate every field into a core value type before touching secrets.
    let email = EmailAddress::parse(&input.email)?;
    let account_auth = std::mem::take(&mut input.auth).into_account_auth()?;
    let incoming_endpoint =
        ServerEndpoint::new(input.incoming.host, nonzero_port(input.incoming.port)?)?;
    let outgoing_endpoint =
        ServerEndpoint::new(input.outgoing.host, nonzero_port(input.outgoing.port)?)?;

    // 2. Derive credential references from the id the row will receive. The
    //    prediction is verified against the assigned id in `finish_account`.
    let account_id = AccountId::new(next_account_id(store).await?);
    let incoming_ref = credential_ref_for_account(account_id, Direction::Incoming);
    let outgoing_ref = credential_ref_for_account(account_id, Direction::Outgoing);

    // 3. Store passwords now for password accounts. OAuth2 accounts have no
    //    secret yet — tokens are stored later by the browser sign-in flow
    //    under these same references.
    let stored_secrets = matches!(account_auth, AccountAuth::Password);
    if stored_secrets {
        set_secret(secrets, incoming_ref.clone(), input.incoming.password).await?;
        if let Err(error) = set_secret(secrets, outgoing_ref.clone(), input.outgoing.password).await
        {
            delete_secret_best_effort(secrets, &incoming_ref).await;
            return Err(error);
        }
    }

    // 4. Insert the account row referencing the stored secrets.
    let new_account = NewAccount {
        display_name: input.display_name,
        email,
        auth: account_auth,
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
            if stored_secrets {
                delete_secret_best_effort(secrets, &incoming_ref).await;
                delete_secret_best_effort(secrets, &outgoing_ref).await;
            }
            return Err(error);
        }
    };

    // 5. Seed mailboxes and build the DTO; roll everything back on failure.
    match finish_account(store, &account, account_id).await {
        Ok(summary) => Ok(summary),
        Err(error) => {
            let _ = store.accounts().delete(account.id).await;
            if stored_secrets {
                delete_secret_best_effort(secrets, &incoming_ref).await;
                delete_secret_best_effort(secrets, &outgoing_ref).await;
            }
            Err(error)
        }
    }
}

/// Lists every configured account as an editable detail view (no secrets).
///
/// This backs the account-management settings section: it carries each
/// account's display name, primary address, protocol, and both servers'
/// host/port/security, but never a password or credential reference.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Configuration`] if a stored id is out of the safe integer
/// range.
pub async fn list_account_details_impl(store: &Store) -> Result<Vec<AccountDetailDto>, MailError> {
    let accounts = store.accounts().list().await?;
    accounts.iter().map(AccountDetailDto::from_domain).collect()
}

/// Applies an account edit: validates the changed transport fields, persists
/// them, and returns the refreshed detail view.
///
/// Only the display name and each server's host/port/security are written.
/// The protocol, login names, primary address, and stored credentials are
/// left untouched — this path can neither switch protocol nor re-key the
/// keychain, so it performs no keychain I/O at all.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an invalid host, a zero port, or
/// an unknown account id, and [`MailError::Database`] on any query failure.
pub async fn update_account_impl(
    store: &Store,
    input: UpdateAccountInput,
) -> Result<AccountDetailDto, MailError> {
    // Validate the edited transport fields into core value types first, so a
    // bad host or port is rejected before the row is touched.
    let incoming_endpoint =
        ServerEndpoint::new(input.incoming.host, nonzero_port(input.incoming.port)?)?;
    let outgoing_endpoint =
        ServerEndpoint::new(input.outgoing.host, nonzero_port(input.outgoing.port)?)?;

    let update = AccountUpdate {
        display_name: input.display_name,
        incoming_endpoint,
        incoming_security: input.incoming.security,
        outgoing_endpoint,
        outgoing_security: input.outgoing.security,
    };
    let account = store
        .accounts()
        .update(AccountId::new(input.account_id), &update)
        .await?;
    AccountDetailDto::from_domain(&account)
}

/// Deletes an account, its cached raw message bodies, and its keychain secrets.
///
/// The account's cached raw `.eml` files live outside the database keyed by
/// message id, so they are enumerated and removed **before** the row deletion
/// cascades the message rows away (otherwise their ids — and thus their paths —
/// would be unrecoverable). Raw-file removal, like secret deletion, is
/// best-effort: filesystem or keychain errors are logged, never surfaced, so a
/// locked or already-missing file cannot block account removal.
///
/// Secret deletion covers both the per-direction password references and the
/// account-scoped OAuth token reference, and is idempotent.
///
/// # Errors
///
/// Returns [`MailError::Database`] if the row deletion query fails.
pub async fn delete_account_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    raw_root: &Path,
    account_id: i64,
) -> Result<(), MailError> {
    let id = AccountId::new(account_id);

    // Capture the message ids while their rows still exist. A query failure
    // here must not block deletion, so it degrades to "no raw files to clean".
    let message_ids = match store.messages().list_ids_for_account(id).await {
        Ok(ids) => ids,
        Err(error) => {
            tracing::warn!(account_id, %error, "could not enumerate messages for raw-cache cleanup");
            Vec::new()
        }
    };
    for message_id in &message_ids {
        remove_raw_best_effort(raw_root, message_id.value()).await;
    }

    store.accounts().delete(id).await?;
    for direction in [Direction::Incoming, Direction::Outgoing] {
        delete_secret_best_effort(secrets, &credential_ref_for_account(id, direction)).await;
    }
    delete_secret_best_effort(secrets, &credential_ref_for_oauth(id)).await;
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
    seed_default_identity(store, account).await?;
    let summaries = mailboxes
        .iter()
        .map(|mailbox| MailboxSummaryDto::from_domain(mailbox, 0))
        .collect::<Result<Vec<_>, _>>()?;
    AccountSummaryDto::new(account, summaries)
}

/// Seeds the account's default sender identity from its own address and
/// display name, so a new account can compose immediately.
async fn seed_default_identity(store: &Store, account: &Account) -> Result<(), MailError> {
    store
        .identities()
        .insert(&NewIdentity {
            account: account.id,
            fields: IdentityFields {
                email: account.email.as_str().to_owned(),
                display_name: account.display_name.clone(),
                reply_to: None,
                signature_text: None,
                signature_placement: SignaturePlacement::BelowQuote,
                reply_position: ReplyPosition::Below,
                forward_mode: ForwardMode::Inline,
                archive_granularity: ArchiveGranularity::Flat,
                sent_dest: None,
                drafts_dest: None,
                templates_dest: None,
                archive_dest: None,
            },
            make_default: true,
            sort_position: 0,
        })
        .await?;
    Ok(())
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
    delete_account_impl(state.store(), state.secrets(), state.raw_root(), account_id)
        .await
        .map_err(map_command_error)?;
    // Drop the removed account's inbox from the background-sync target set.
    refresh_scheduler_targets(&state).await;
    Ok(())
}

/// Lists every configured account with its editable server configuration.
#[tauri::command]
pub async fn list_account_details(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AccountDetailDto>, String> {
    list_account_details_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Updates an account's display name and server transport settings.
#[tauri::command]
pub async fn update_account(
    state: tauri::State<'_, AppState>,
    input: UpdateAccountInput,
) -> Result<AccountDetailDto, String> {
    update_account_impl(state.store(), input)
        .await
        .map_err(map_command_error)
}

/// Pushes the current inbox target set into the scheduler, logging (never
/// surfacing) a failure: the account change already succeeded.
async fn refresh_scheduler_targets(state: &AppState) {
    if let Err(error) = state.scheduler().refresh_targets().await {
        tracing::warn!(error = %error, "could not refresh background-sync targets");
    }
}
