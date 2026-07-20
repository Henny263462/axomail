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
    Account, AccountId, Address, EmailAddress, FlagSet, IncomingProtocol, MailError, Mailbox,
    MailboxRole, MessageFlags, OutgoingAttachment, OutgoingMessage, SystemFlags, ThreadingHeaders,
    UtcTimestamp,
};
use axomail_smtp::{RenderedMessage, SmtpConfig, SmtpProvider, render_message};
use axomail_storage::{Identity, NewMessage, OutboxId, OutboxRecord, Store};

use super::append::SentCopyPlan;
use crate::composition::resolve_credential;
use crate::dto::SendOutcomeDto;
use crate::state::SharedSecretStore;

/// The result of a submission attempt: the DTO to return to the caller, and —
/// on a successful send that resolved a server Sent folder — a plan to upload
/// the Sent copy on a background task.
#[derive(Debug)]
pub struct Submission {
    /// Outcome to hand back to the composer (sent / failed / scheduled).
    pub outcome: SendOutcomeDto,
    /// Present only when an IMAP Sent folder was resolved: the interactive
    /// command wrappers spawn the server `APPEND` from it.
    pub sent_copy: Option<SentCopyPlan>,
}

/// Length at which the stored Sent-copy preview is truncated.
const PREVIEW_LEN: usize = 200;

/// Per-send content that rides only on the immediate send path.
///
/// The durable outbox row is text-only (no migration this wave), so the reply
/// threading headers, the HTML alternative, and any attachments (picked files
/// and a forwarded `message/rfc822` part) are threaded in-memory into the one
/// live submission rather than persisted. A retry (which rebuilds from the
/// stored row) or a deferred send therefore uses [`MessageExtras::default`] —
/// the plain-text alternative — never a silently stripped copy. A scheduled or
/// offline reply consequently sends without its threading headers, a documented
/// limitation; a deferred send carrying attachments is rejected up front.
#[derive(Debug, Default)]
pub(super) struct MessageExtras {
    /// `In-Reply-To` (source Message-ID, no angle brackets).
    pub in_reply_to: Option<String>,
    /// `References` chain (oldest first).
    pub references: Vec<String>,
    /// Sanitized HTML alternative, or `None` for a text-only message.
    pub html_body: Option<String>,
    /// Attachments to include: picked files read from disk and any forwarded
    /// `message/rfc822` parts, each with a sanitized name and MIME type.
    pub attachments: Vec<OutgoingAttachment>,
    /// `Disposition-Notification-To` value (the read-receipt request address)
    /// when the composer asked for a receipt; `None` requests no receipt. Like
    /// the other extras it rides only the immediate-send path.
    pub disposition_notification_to: Option<String>,
}

/// Submits one outbox row (already transitioned to `sending`) over SMTP and
/// records the terminal state. Never propagates a send failure as an error:
/// it is captured on the row and in the returned outcome.
///
/// The message is rendered once so the identical bytes go to SMTP and to the
/// local Sent copy; on success a [`SentCopyPlan`] is returned (for IMAP accounts
/// with a resolvable Sent folder) so the caller can upload the copy to the
/// server on a background task. `extras` carries the threading headers, HTML
/// alternative, and attachments for a first, immediate send; retries pass
/// [`MessageExtras::default`].
pub(super) async fn submit_record(
    store: &Store,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
    account: &Account,
    record: &OutboxRecord,
    extras: MessageExtras,
) -> Result<Submission, MailError> {
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
    // Render once: SMTP delivery and the Sent copy share these exact bytes and
    // their single Message-ID.
    let rendered = match render_message(&message) {
        Ok(rendered) => rendered,
        Err(error) => return fail(store, record.id, error).await,
    };

    match provider.send_body(&message, &rendered.bytes).await {
        Ok(_) => {
            store.outbox().mark_sent(record.id).await?;
            let sent_copy = file_sent_copy(
                store,
                account,
                identity.as_ref(),
                record,
                &message,
                rendered,
            )
            .await;
            Ok(Submission {
                outcome: SendOutcomeDto::new(record.id.value(), true, None)?,
                sent_copy,
            })
        }
        Err(error) => fail(store, record.id, error).await,
    }
}

/// Records a failed submission and reports it in the outcome (never as an
/// error): `MailError` messages are redacted by construction (ADR-0003).
async fn fail(store: &Store, id: OutboxId, error: MailError) -> Result<Submission, MailError> {
    let reason = error.to_string();
    store.outbox().mark_failed(id, &reason).await?;
    Ok(Submission {
        outcome: SendOutcomeDto::new(id.value(), false, Some(reason))?,
        sent_copy: None,
    })
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
/// identity, and the immediate-send `extras` (threading headers, HTML
/// alternative, and attachments). The identity, when present, supplies the From
/// and Reply-To; otherwise the account's own address is used. The plain-text
/// body always comes from the durable row; the `extras` are not persisted on
/// the outbox row (see [`MessageExtras`]).
fn build_message(
    account: &Account,
    record: &OutboxRecord,
    identity: Option<&Identity>,
    extras: MessageExtras,
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
        html_body: extras.html_body,
        attachments: extras.attachments,
        in_reply_to: extras.in_reply_to,
        references: extras.references,
        disposition_notification_to: extras.disposition_notification_to,
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

/// Files a local copy of a sent message in the account's Sent mailbox and, for
/// an IMAP account, returns a [`SentCopyPlan`] to upload the copy to the server.
///
/// The Sent folder is resolved from the sending identity's configured
/// `sent_dest`, falling back to the account's `\Sent`-role mailbox. A missing
/// Sent mailbox is logged and skipped — the message was already delivered, so
/// this must never fail. POP3 accounts have no server folder, so they file
/// locally only and return `None`.
async fn file_sent_copy(
    store: &Store,
    account: &Account,
    identity: Option<&Identity>,
    record: &OutboxRecord,
    message: &OutgoingMessage,
    rendered: RenderedMessage,
) -> Option<SentCopyPlan> {
    let sent = match resolve_sent_mailbox(store, account, identity).await {
        Ok(Some(mailbox)) => mailbox,
        Ok(None) => {
            tracing::warn!("account has no Sent mailbox; skipping local sent copy");
            return None;
        }
        Err(error) => {
            tracing::warn!(error = %error, "could not resolve Sent mailbox for sent copy");
            return None;
        }
    };
    let size = u64::try_from(rendered.bytes.len()).ok();
    let new = NewMessage {
        mailbox: sent.id,
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
        size_bytes: size,
        preview: Some(preview_of(&record.body_text)),
        has_raw: false,
    };
    let local_copy_id = match store.messages().insert(&new).await {
        Ok(envelope) => envelope.id,
        Err(error) => {
            tracing::warn!(error = %error, "could not file local sent copy");
            return None;
        }
    };
    // Record the sent message's Message-ID so it threads with later replies and
    // so a synced server copy can be matched back to this local row.
    let headers = ThreadingHeaders {
        message_id: Some(rendered.message_id),
        in_reply_to: Vec::new(),
        references: Vec::new(),
    };
    if let Err(error) = store
        .messages()
        .backfill_threading(local_copy_id, &headers)
        .await
    {
        tracing::warn!(error = %error, "could not record the sent copy's Message-ID");
    }

    // Only IMAP accounts have a server-side Sent folder to append into.
    if !matches!(account.incoming.protocol, IncomingProtocol::Imap) {
        return None;
    }
    Some(SentCopyPlan {
        account_id: account.id,
        local_copy_id,
        mailbox_remote: sent.remote_name,
        flags: FlagSet::from_system(SystemFlags {
            seen: true,
            ..SystemFlags::default()
        }),
        raw: rendered.bytes,
    })
}

/// Resolves the mailbox that sent copies are filed into: the sending identity's
/// configured `sent_dest` when it still exists, otherwise the account's
/// `\Sent`-role mailbox.
async fn resolve_sent_mailbox(
    store: &Store,
    account: &Account,
    identity: Option<&Identity>,
) -> Result<Option<Mailbox>, MailError> {
    let mailboxes = store.mailboxes().list(account.id).await?;
    if let Some(dest) = identity.and_then(|identity| identity.fields.sent_dest)
        && let Some(mailbox) = mailboxes.iter().find(|mailbox| mailbox.id == dest)
    {
        return Ok(Some(mailbox.clone()));
    }
    Ok(mailboxes
        .into_iter()
        .find(|mailbox| mailbox.role == MailboxRole::Sent))
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
