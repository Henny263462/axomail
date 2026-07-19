//! Shared submission pipeline and helpers for the compose commands.
//!
//! [`submit_record`] takes an outbox row already transitioned to `sending`,
//! builds the message and provider, submits over SMTP, and records the terminal
//! state — the code path both `send_message` (first attempt) and `retry_outbox`
//! (re-run) funnel through. The remaining functions are the small pieces they
//! both reuse: account/address loading, the SMTP provider builder, the
//! message assembler, and the local Sent-copy filer.

use std::time::{SystemTime, UNIX_EPOCH};

use axomail_core::{
    Account, AccountId, Address, EmailAddress, MailError, MailboxId, MailboxRole, MessageFlags,
    OutgoingAttachment, OutgoingMailProvider, OutgoingMessage, UtcTimestamp,
};
use axomail_smtp::{SmtpConfig, SmtpProvider};
use axomail_storage::{Identity, NewMessage, OutboxId, OutboxRecord, Store};

use crate::composition::resolve_credential;
use crate::dto::SendOutcomeDto;
use crate::state::SharedSecretStore;

/// Length at which the stored Sent-copy preview is truncated.
const PREVIEW_LEN: usize = 200;

/// Per-send data that is not persisted on the outbox row.
///
/// Threading headers (reply) and a forwarded `message/rfc822` part live here
/// because the outbox schema carries no columns for them (no migration this
/// wave). They are supplied on the immediate-send path from the compose input;
/// the retry/drain path submits with [`SendExtras::default`], so a scheduled or
/// offline reply/forward sends without these extras — a documented limitation.
#[derive(Debug, Default, Clone)]
pub(super) struct SendExtras {
    /// `In-Reply-To` (source Message-ID, no angle brackets).
    pub in_reply_to: Option<String>,
    /// `References` chain (oldest first).
    pub references: Vec<String>,
    /// Attachments to include on this message.
    pub attachments: Vec<OutgoingAttachment>,
}

/// Submits one outbox row (already transitioned to `sending`) over SMTP and
/// records the terminal state. Never propagates a send failure as an error:
/// it is captured on the row and in the returned outcome.
pub(super) async fn submit_record(
    store: &Store,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
    account: &Account,
    record: &OutboxRecord,
    extras: &SendExtras,
) -> Result<SendOutcomeDto, MailError> {
    // Resolve the identity the row was queued under; a deleted identity nulls
    // the reference (ON DELETE SET NULL), so this falls back to the account.
    let identity = match record.identity {
        Some(id) => store.identities().get(id).await?,
        None => None,
    };
    let message = match build_message(account, record, identity.as_ref(), extras) {
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

/// Assembles the outgoing message from the account, stored row, chosen
/// identity, and per-send extras. The identity, when present, supplies the From
/// and Reply-To; otherwise the account's own address is used. `extras` carries
/// the threading headers and any attachments (e.g. a forwarded message), which
/// are not persisted on the outbox row (see [`SendExtras`]).
fn build_message(
    account: &Account,
    record: &OutboxRecord,
    identity: Option<&Identity>,
    extras: &SendExtras,
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
        attachments: extras.attachments.clone(),
        in_reply_to: extras.in_reply_to.clone(),
        references: extras.references.clone(),
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
async fn sent_mailbox(store: &Store, account: AccountId) -> Result<Option<MailboxId>, MailError> {
    Ok(store
        .mailboxes()
        .list(account)
        .await?
        .into_iter()
        .find(|mailbox| mailbox.role == MailboxRole::Sent)
        .map(|mailbox| mailbox.id))
}

/// Loads an account by id, mapping an unknown id to a configuration error.
pub(super) async fn load_account(store: &Store, account_id: i64) -> Result<Account, MailError> {
    store
        .accounts()
        .get(AccountId::new(account_id))
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the message references an account that does not exist".to_owned(),
        })
}

/// Parses a list of address strings, failing on the first invalid one.
pub(super) fn parse_addresses(addresses: &[String]) -> Result<Vec<Address>, MailError> {
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
pub(super) fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
