//! The send/outbox commands: `send_message`, `retry_outbox`, `list_outbox`.
//!
//! A send always goes through the outbox: the message is queued, moved into
//! `sending` by a guarded transition (the duplicate-send guard), submitted
//! over SMTP, and then recorded as `sent` or `failed`. Permanent failures are
//! never retried automatically — only the explicit `retry_outbox` command
//! re-runs the flow, and its guarded transition means a row already `sending`
//! or `sent` is never submitted twice.

use std::time::{SystemTime, UNIX_EPOCH};

use axomail_core::{
    Account, AccountId, Address, EmailAddress, MailError, MailboxRole, MessageFlags,
    OutgoingMailProvider, OutgoingMessage, UtcTimestamp,
};
use axomail_smtp::{SmtpConfig, SmtpProvider};
use axomail_storage::{
    Identity, IdentityId, NewMessage, NewOutbox, OutboxId, OutboxRecord, OutboxState, Store,
};

use crate::commands::map_command_error;
use crate::composition::resolve_credential;
use crate::dto::{ComposeInput, OutboxItemDto, SendOutcomeDto};
use crate::state::{AppState, SharedSecretStore};

/// Length at which the stored Sent-copy preview is truncated.
const PREVIEW_LEN: usize = 200;

/// Sends a composed message through the outbox.
///
/// Recipients are validated up front, so an invalid address fails the
/// command before any row is written. On success the outbox row is marked
/// `sent` and a local copy is filed in the account's Sent mailbox.
///
/// `extra_roots` is empty in production; tests inject a local server's
/// self-signed certificate through it.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an empty or invalid recipient
/// set or an unknown account. A submission failure is reported inside the
/// returned [`SendOutcomeDto`] (with a redacted reason), not as an error.
pub async fn send_message_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
    input: ComposeInput,
    offline: bool,
) -> Result<SendOutcomeDto, MailError> {
    if input.to.is_empty() {
        return Err(MailError::Configuration {
            message: "a message needs at least one primary (To) recipient".to_owned(),
        });
    }
    // Validate every address before writing anything.
    parse_addresses(&input.to)?;
    parse_addresses(&input.cc)?;
    parse_addresses(&input.bcc)?;

    let account = load_account(store, input.account_id).await?;
    let identity = resolve_identity(store, &account, input.identity_id).await?;
    // A schedule strictly in the future defers the send; a past/now instant is
    // treated as "send now".
    let scheduled = input.scheduled_at_unix.filter(|&at| at > now_unix());

    let record = store
        .outbox()
        .create(&NewOutbox {
            account: account.id,
            draft_id: None,
            identity_id: identity.as_ref().map(|id| id.id),
            subject: input.subject,
            body_text: input.text_body,
            to: input.to,
            cc: input.cc,
            bcc: input.bcc,
            next_attempt_unix: scheduled,
        })
        .await?;

    // A scheduled message waits in the outbox; the worker's time guard keeps it
    // from being submitted until it is due.
    if let Some(at) = scheduled {
        return SendOutcomeDto::scheduled(record.id.value(), at);
    }

    // Working offline: the message stays `queued` in the outbox and is sent by
    // the drain once the app is back online. No network I/O happens now.
    if offline {
        return SendOutcomeDto::queued_offline(record.id.value());
    }

    // A freshly created row is `queued`, so this transition always succeeds.
    if !store.outbox().begin_send(record.id).await? {
        return SendOutcomeDto::new(
            record.id.value(),
            false,
            Some("the message could not be queued for sending".to_owned()),
        );
    }
    submit_record(store, secrets, extra_roots, &account, &record).await
}

/// Loads and validates the chosen sender identity, if any.
///
/// Returns `Ok(None)` when no identity was selected (the account address is
/// used). Errors if the identity does not exist or belongs to another account.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown or mismatched identity.
async fn resolve_identity(
    store: &Store,
    account: &Account,
    identity_id: Option<i64>,
) -> Result<Option<Identity>, MailError> {
    let Some(identity_id) = identity_id else {
        return Ok(None);
    };
    let identity = store
        .identities()
        .get(IdentityId::new(identity_id))
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the message references an identity that does not exist".to_owned(),
        })?;
    if identity.account != account.id {
        return Err(MailError::Configuration {
            message: "the chosen identity does not belong to the sending account".to_owned(),
        });
    }
    Ok(Some(identity))
}

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
    submit_record(store, secrets, extra_roots, &account, &record).await
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

/// Submits one outbox row (already transitioned to `sending`) over SMTP and
/// records the terminal state. Never propagates a send failure as an error:
/// it is captured on the row and in the returned outcome.
async fn submit_record(
    store: &Store,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
    account: &Account,
    record: &OutboxRecord,
) -> Result<SendOutcomeDto, MailError> {
    // Resolve the identity the row was queued under; a deleted identity nulls
    // the reference (ON DELETE SET NULL), so this falls back to the account.
    let identity = match record.identity {
        Some(id) => store.identities().get(id).await?,
        None => None,
    };
    let message = match build_message(account, record, identity.as_ref()) {
        Ok(message) => message,
        Err(error) => return fail(store, record.id, error).await,
    };
    let provider = match build_provider(secrets, extra_roots, account).await {
        Ok(provider) => provider,
        Err(error) => return fail(store, record.id, error).await,
    };

    match provider.send(&message).await {
        Ok(_) => {
            store.outbox().mark_sent(record.id).await?;
            file_sent_copy(store, account, record, &message).await;
            SendOutcomeDto::new(record.id.value(), true, None)
        }
        Err(error) => fail(store, record.id, error).await,
    }
}

/// Records a failed submission and reports it in the outcome (never as an
/// error): `MailError` messages are redacted by construction (ADR-0003).
async fn fail(store: &Store, id: OutboxId, error: MailError) -> Result<SendOutcomeDto, MailError> {
    let reason = error.to_string();
    store.outbox().mark_failed(id, &reason).await?;
    SendOutcomeDto::new(id.value(), false, Some(reason))
}

/// Builds the SMTP provider for an account, resolving its outgoing secret.
async fn build_provider(
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
    account: &Account,
) -> Result<SmtpProvider, MailError> {
    let credential = resolve_credential(secrets, account, &account.outgoing.credential).await?;
    let mut config = SmtpConfig::with_credential(
        account.outgoing.endpoint.clone(),
        account.outgoing.security,
        account.outgoing.username.clone(),
        credential,
    );
    config.extra_root_certs_der = extra_roots.to_vec();
    SmtpProvider::new(config)
}

/// Assembles the outgoing message from the account, stored row, and chosen
/// identity (text only per the composer contract; attachments and HTML come
/// later). The identity, when present, supplies the From and Reply-To;
/// otherwise the account's own address is used.
fn build_message(
    account: &Account,
    record: &OutboxRecord,
    identity: Option<&Identity>,
) -> Result<OutgoingMessage, MailError> {
    let (from, reply_to) = sender_headers(account, identity)?;
    Ok(OutgoingMessage {
        from,
        to: parse_addresses(&record.to)?,
        cc: parse_addresses(&record.cc)?,
        bcc: parse_addresses(&record.bcc)?,
        reply_to,
        subject: record.subject.clone(),
        text_body: record.body_text.clone(),
        html_body: None,
        attachments: Vec::new(),
    })
}

/// Derives the From address and optional Reply-To for a message, preferring the
/// chosen identity over the account's own address.
fn sender_headers(
    account: &Account,
    identity: Option<&Identity>,
) -> Result<(Address, Option<Address>), MailError> {
    let Some(identity) = identity else {
        let from = Address {
            display_name: Some(account.display_name.clone()),
            email: account.email.clone(),
        };
        return Ok((from, None));
    };
    let from = Address {
        display_name: (!identity.fields.display_name.is_empty())
            .then(|| identity.fields.display_name.clone()),
        email: EmailAddress::parse(&identity.fields.email)?,
    };
    let reply_to = match identity.fields.reply_to.as_deref() {
        Some(value) if !value.trim().is_empty() => {
            Some(Address::from_email(EmailAddress::parse(value.trim())?))
        }
        _ => None,
    };
    Ok((from, reply_to))
}

/// Files a local copy of a sent message in the account's Sent mailbox.
///
/// This is local bookkeeping only; uploading the copy to the server via IMAP
/// `APPEND` is a later improvement. A missing Sent mailbox is logged and
/// skipped — the message was already delivered, so this must not fail.
async fn file_sent_copy(
    store: &Store,
    account: &Account,
    record: &OutboxRecord,
    message: &OutgoingMessage,
) {
    let sent = match sent_mailbox(store, account.id).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            tracing::warn!("account has no Sent mailbox; skipping local sent copy");
            return;
        }
        Err(error) => {
            tracing::warn!(error = %error, "could not resolve Sent mailbox for sent copy");
            return;
        }
    };
    let new = NewMessage {
        mailbox: sent,
        remote_id: None,
        subject: (!record.subject.is_empty()).then(|| record.subject.clone()),
        from: vec![message.from.clone()],
        to: message.to.clone(),
        cc: message.cc.clone(),
        bcc: message.bcc.clone(),
        date: Some(UtcTimestamp::from_unix_seconds(now_unix())),
        flags: MessageFlags {
            seen: true,
            ..MessageFlags::default()
        },
        size_bytes: None,
        preview: Some(preview_of(&record.body_text)),
        has_raw: false,
    };
    if let Err(error) = store.messages().insert(&new).await {
        tracing::warn!(error = %error, "could not file local sent copy");
    }
}

/// Finds the id of the account's Sent mailbox, if it has one.
async fn sent_mailbox(
    store: &Store,
    account: AccountId,
) -> Result<Option<axomail_core::MailboxId>, MailError> {
    Ok(store
        .mailboxes()
        .list(account)
        .await?
        .into_iter()
        .find(|mailbox| mailbox.role == MailboxRole::Sent)
        .map(|mailbox| mailbox.id))
}

/// Loads an account by id, mapping an unknown id to a configuration error.
async fn load_account(store: &Store, account_id: i64) -> Result<Account, MailError> {
    store
        .accounts()
        .get(AccountId::new(account_id))
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the message references an account that does not exist".to_owned(),
        })
}

/// Parses a list of address strings, failing on the first invalid one.
fn parse_addresses(addresses: &[String]) -> Result<Vec<Address>, MailError> {
    addresses
        .iter()
        .map(|value| EmailAddress::parse(value).map(Address::from_email))
        .collect()
}

/// Builds a short, plain-text preview from a body.
fn preview_of(body: &str) -> String {
    let collapsed: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(PREVIEW_LEN).collect()
}

/// Current time in Unix seconds, or `0` if the clock predates the epoch.
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Sends a composed message, queuing it in the outbox first.
#[tauri::command]
pub async fn send_message(
    state: tauri::State<'_, AppState>,
    input: ComposeInput,
) -> Result<SendOutcomeDto, String> {
    send_message_impl(
        state.store(),
        state.secrets(),
        &[],
        input,
        state.is_offline(),
    )
    .await
    .map_err(map_command_error)
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
