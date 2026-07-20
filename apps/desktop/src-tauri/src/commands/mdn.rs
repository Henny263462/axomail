//! Read receipts (MDN, RFC 8098): the reader-side `send_mdn` command and the
//! receipt-policy settings pair.
//!
//! [`send_mdn`] honours a received message's `Disposition-Notification-To`
//! request. It authoritatively re-decides the policy server-side (never trusting
//! the caller), builds the disposition-notification report through the SMTP
//! plumbing, delivers it, files a local copy in Sent, records the local
//! `$MDNSent` marker so it is never sent twice, and — for an IMAP account —
//! enqueues the `$MDNSent` keyword write-back via the Wave-2 op queue.

use std::path::Path;

use axomail_core::{
    Account, Address, IncomingMailProvider, IncomingProtocol, Keyword, MailError, MailboxRole,
    MessageEnvelope, MessageFlags, MessageId, OutgoingMessage, RemoteMessageId, UtcTimestamp,
};
use axomail_parser::{ParsedMessage, parse_message};
use axomail_smtp::{MdnDisposition, MdnReport, SmtpConfig, SmtpProvider, render_mdn};
use axomail_storage::{NewMessage, NewPendingOp, Store, raw_message_path, read_raw};
use axomail_sync::{ProviderFactory, SyncEngine, WorkRequest, keyword_op};

use super::detail::fetch_and_cache;
use super::map_command_error;
use crate::composition::resolve_credential;
use crate::dto::{MdnOutcomeDto, ReceiptSettingsDto};
use crate::receipts::{self, ReceiptAction};
use crate::state::{AppState, SharedSecretStore};

/// Thunderbird-compatible keyword marking that a receipt was sent for a message.
const MDN_SENT_KEYWORD: &str = "$MDNSent";

/// Sends a read receipt for a message that requested one, honouring the policy.
///
/// `extra_roots` is empty in production; tests inject the local SMTP server's
/// certificate through it.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] when the message is unknown, has no
/// cached body (and none can be fetched), or did not request a receipt.
/// Delivery failures are captured in the returned [`MdnOutcomeDto`], not raised.
// Wires the store, the two provider surfaces (engine for op-replay, factory for
// the on-demand source fetch), the SMTP secret store, the raw cache root, and
// the test-only extra roots — each a distinct resource, so they are passed
// individually (as the reply/forward source-marking command does).
#[allow(clippy::too_many_arguments)]
pub async fn send_mdn_impl<P, F>(
    store: &Store,
    secrets: &SharedSecretStore,
    engine: &SyncEngine<P, F>,
    factory: &F,
    raw_root: &Path,
    extra_roots: &[Vec<u8>],
    message_id: i64,
    offline: bool,
) -> Result<MdnOutcomeDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let id = MessageId::new(message_id);
    let envelope = store
        .messages()
        .get(id)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the message to acknowledge no longer exists".to_owned(),
        })?;

    // Idempotent: a receipt is sent at most once per message.
    if receipts::mdn_already_sent(store, message_id).await? {
        return Ok(MdnOutcomeDto::already_sent());
    }

    let account = account_of(store, &envelope).await?;
    let bytes = load_source_bytes(store, factory, raw_root, &envelope, offline).await?;
    let parsed = parse_message(&bytes)?;

    let Some(requester) = parsed.disposition_notification_to.first().cloned() else {
        return Err(MailError::Configuration {
            message: "this message did not request a read receipt".to_owned(),
        });
    };

    // Re-decide the policy server-side; never trust the caller to have done so.
    let mine = receipts::self_emails(store, &account).await?;
    let addressed_to_me = parsed
        .to
        .iter()
        .chain(&parsed.cc)
        .any(|address| mine.contains(&address.email.as_str().to_ascii_lowercase()));
    let action = receipts::decide(
        receipts::settings(store).await?,
        addressed_to_me,
        parsed.mailing_list,
    );
    if action == ReceiptAction::Ignore {
        return Ok(MdnOutcomeDto::suppressed());
    }
    let disposition = match action {
        ReceiptAction::Send => MdnDisposition::Automatic,
        _ => MdnDisposition::Manual,
    };

    let report = build_report(&account, &parsed, &requester, &mine, disposition);
    let rendered = render_mdn(&report)?;

    let provider = build_provider(secrets, extra_roots, &account).await?;
    let envelope_message = envelope_for(&report);

    // Atomically claim the send just before delivering: the "already sent?"
    // check and the marker write are a single atomic operation, so two
    // concurrent `send_mdn` calls for the same message cannot both send — exactly
    // one wins the claim and sends. A loser reports it was already sent.
    if !receipts::claim_mdn_send(store, message_id).await? {
        return Ok(MdnOutcomeDto::already_sent());
    }
    if let Err(error) = provider.send_body(&envelope_message, &rendered.bytes).await {
        // The send failed: release the claim so a later retry may send.
        receipts::release_mdn_claim(store, message_id).await?;
        return Ok(MdnOutcomeDto::failed(error.to_string()));
    }

    // Delivered (the claim already recorded $MDNSent locally): file a local copy
    // and propagate the keyword.
    file_receipt_copy(store, &account, &report, &rendered.bytes).await;
    propagate_keyword(store, engine, &account, &envelope, id).await?;
    Ok(MdnOutcomeDto::sent())
}

/// Loads the account owning the message's mailbox.
async fn account_of(store: &Store, envelope: &MessageEnvelope) -> Result<Account, MailError> {
    let mailbox = store
        .mailboxes()
        .get(envelope.mailbox)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the message's mailbox no longer exists".to_owned(),
        })?;
    store
        .accounts()
        .get(mailbox.account)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the message's account no longer exists".to_owned(),
        })
}

/// Loads the source `.eml` from the raw cache, fetching it on demand when
/// online. Mirrors the reply/forward loader.
async fn load_source_bytes<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    envelope: &MessageEnvelope,
    offline: bool,
) -> Result<Vec<u8>, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let path = raw_message_path(raw_root, envelope.id.value());
    if let Some(bytes) = read_raw(&path).await? {
        return Ok(bytes);
    }
    if envelope.remote_id.is_none() {
        return Err(MailError::Configuration {
            message: "this message has no cached body to acknowledge".to_owned(),
        });
    }
    if offline {
        return Err(MailError::Configuration {
            message: "this message is not available offline; open it while online first".to_owned(),
        });
    }
    fetch_and_cache(store, factory, &envelope.remote_id, envelope.mailbox, &path).await
}

/// Assembles the [`MdnReport`]: the receipt is sent from the account's own
/// address that received the message (falling back to the account address),
/// back to the requester.
fn build_report(
    account: &Account,
    parsed: &ParsedMessage,
    requester: &Address,
    mine: &std::collections::HashSet<String>,
    disposition: MdnDisposition,
) -> MdnReport {
    // The address the message was delivered to (as far as we can tell): the
    // first of our own addresses that appears in To/Cc, else the account's.
    let final_recipient = parsed
        .to
        .iter()
        .chain(&parsed.cc)
        .find(|address| mine.contains(&address.email.as_str().to_ascii_lowercase()))
        .map_or_else(
            || account.email.as_str().to_owned(),
            |address| address.email.as_str().to_owned(),
        );
    let from = if final_recipient.eq_ignore_ascii_case(account.email.as_str()) {
        Address {
            display_name: Some(account.display_name.clone()),
            email: account.email.clone(),
        }
    } else {
        Address::from_email(account.email.clone())
    };
    MdnReport {
        from,
        to: requester.clone(),
        subject: parsed.subject.clone().unwrap_or_default(),
        original_message_id: parsed.message_id.clone(),
        final_recipient,
        disposition,
    }
}

/// A minimal outgoing message carrying only the SMTP envelope (from + the sole
/// recipient) for [`SmtpProvider::send_body`]; the body bytes are the rendered
/// report.
fn envelope_for(report: &MdnReport) -> OutgoingMessage {
    OutgoingMessage {
        from: report.from.clone(),
        to: vec![report.to.clone()],
        cc: Vec::new(),
        bcc: Vec::new(),
        reply_to: None,
        subject: String::new(),
        text_body: String::new(),
        html_body: None,
        attachments: Vec::new(),
        in_reply_to: None,
        references: Vec::new(),
        disposition_notification_to: None,
    }
}

/// Builds the SMTP provider for an account (identical to the compose path).
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

/// Files the sent receipt into the account's Sent mailbox (best-effort: the
/// receipt was already delivered, so a filing failure is logged, never raised).
async fn file_receipt_copy(store: &Store, account: &Account, report: &MdnReport, raw: &[u8]) {
    let sent = match store.mailboxes().list(account.id).await {
        Ok(mailboxes) => mailboxes
            .into_iter()
            .find(|mailbox| mailbox.role == MailboxRole::Sent),
        Err(error) => {
            tracing::warn!(error = %error, "could not resolve Sent mailbox for the receipt copy");
            return;
        }
    };
    let Some(sent) = sent else {
        return;
    };
    let new = NewMessage {
        mailbox: sent.id,
        remote_id: None,
        subject: Some(format!("Read: {}", report.subject)),
        from: vec![report.from.clone()],
        to: vec![report.to.clone()],
        cc: Vec::new(),
        bcc: Vec::new(),
        date: Some(UtcTimestamp::from_unix_seconds(now_unix())),
        flags: MessageFlags {
            seen: true,
            ..MessageFlags::default()
        },
        size_bytes: u64::try_from(raw.len()).ok(),
        preview: Some("Return receipt".to_owned()),
        has_raw: false,
    };
    if let Err(error) = store.messages().insert(&new).await {
        tracing::warn!(error = %error, "could not file the sent receipt copy");
    }
}

/// Records `$MDNSent` for propagation: enqueues the additive keyword write and
/// triggers a replay for an IMAP message with a server identity. A POP3 or
/// local-only message is a no-op (the local marker already prevents resends).
async fn propagate_keyword<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    account: &Account,
    envelope: &MessageEnvelope,
    id: MessageId,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    if account.incoming.protocol != IncomingProtocol::Imap
        || !matches!(envelope.remote_id, Some(RemoteMessageId::Imap { .. }))
    {
        return Ok(());
    }
    let (kind, payload_json) = keyword_op(Keyword::parse(MDN_SENT_KEYWORD)?, true);
    store
        .pending_ops()
        .enqueue(&NewPendingOp {
            account_id: account.id,
            message_id: id,
            kind,
            payload_json,
        })
        .await?;
    engine
        .enqueue_work(WorkRequest::OpReplay {
            account_id: account.id,
        })
        .await?;
    Ok(())
}

/// Current time in Unix seconds, or `0` if the clock predates the epoch.
fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Sends a read receipt for a message that requested one.
#[tauri::command]
pub async fn send_mdn(
    state: tauri::State<'_, AppState>,
    message_id: i64,
) -> Result<MdnOutcomeDto, String> {
    send_mdn_impl(
        state.store(),
        state.secrets(),
        state.engine(),
        state.factory(),
        state.raw_root(),
        &[],
        message_id,
        state.is_offline(),
    )
    .await
    .map_err(map_command_error)
}

/// Reads the read-receipt honour settings.
#[tauri::command]
pub async fn get_receipt_settings(
    state: tauri::State<'_, AppState>,
) -> Result<ReceiptSettingsDto, String> {
    receipts::settings(state.store())
        .await
        .map_err(map_command_error)
}

/// Persists the read-receipt honour settings.
#[tauri::command]
pub async fn set_receipt_settings(
    state: tauri::State<'_, AppState>,
    settings: ReceiptSettingsDto,
) -> Result<(), String> {
    receipts::set_settings(state.store(), settings)
        .await
        .map_err(map_command_error)
}
