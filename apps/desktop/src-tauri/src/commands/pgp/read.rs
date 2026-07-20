//! Read-side PGP logic: the composer's sign/encrypt availability, and the
//! reading pane's verify-and-decrypt analysis of a received message.

use std::path::Path;

use axomail_core::{IncomingMailProvider, MailError};
use axomail_parser::parse_message;
use axomail_security::pgp::{
    PgpMimeMessage, SignatureVerification, credential_ref_for_pgp, decrypt_message, parse_pgp_mime,
    verify_detached,
};
use axomail_security::sanitize_message_html;
use axomail_storage::Store;
use axomail_sync::ProviderFactory;

use super::manage::read_secret;
use super::normalize_email;
use crate::commands::detail::load_raw_for_export;
use crate::dto::{PgpComposeStatusDto, PgpMessageStatusDto};
use crate::state::SharedSecretStore;

/// Reports whether an outgoing message from `from_email` to `recipients` can be
/// signed and/or encrypted, and which recipients block encryption.
pub async fn compose_status_impl(
    store: &Store,
    from_email: String,
    recipients: Vec<String>,
) -> Result<PgpComposeStatusDto, MailError> {
    let can_sign = store
        .pgp_keys()
        .own_for_email(&normalize_email(&from_email))
        .await?
        .is_some();

    let mut missing = Vec::new();
    for recipient in &recipients {
        let email = normalize_email(recipient);
        if email.is_empty() {
            continue;
        }
        let has_recipient = store.pgp_keys().has_recipient_key(&email).await?;
        let has_own = store
            .pgp_keys()
            .own_key_for_address(&email)
            .await?
            .is_some();
        if !has_recipient && !has_own {
            missing.push(recipient.clone());
        }
    }
    let can_encrypt = missing.is_empty() && !recipients.is_empty();
    Ok(PgpComposeStatusDto {
        can_sign,
        can_encrypt,
        missing_recipients: missing,
    })
}

/// Analyses a received message's OpenPGP status: recognise the structure,
/// verify a signature, and (with the user-supplied passphrase, never stored)
/// decrypt an encrypted body for the reading pane.
pub(super) async fn analyze_impl<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    secrets: &SharedSecretStore,
    message_id: i64,
    passphrase: Option<String>,
    offline: bool,
) -> Result<PgpMessageStatusDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let raw = load_raw_for_export(store, factory, raw_root, message_id, offline).await?;
    let Some(parsed) = parse_pgp_mime(&raw) else {
        return Ok(PgpMessageStatusDto::not_pgp());
    };

    match parsed {
        PgpMimeMessage::SignedMime {
            signed_entity,
            signature,
        } => verified_status(store, &signed_entity, &signature).await,
        PgpMimeMessage::EncryptedMime { ciphertext }
        | PgpMimeMessage::InlineEncrypted { ciphertext } => {
            decrypt_status(store, secrets, &ciphertext, passphrase.as_deref()).await
        }
    }
}

/// Builds the status for a signed (unencrypted) message: verify and render.
async fn verified_status(
    store: &Store,
    signed_entity: &[u8],
    signature: &str,
) -> Result<PgpMessageStatusDto, MailError> {
    let candidates = candidate_public_keys(store).await?;
    let verification = verify_detached(&candidates, signature, signed_entity)?;
    let (body_html, body_text) = render_entity(signed_entity);
    Ok(PgpMessageStatusDto {
        is_pgp: true,
        signed: true,
        signature_status: status_label(verification).to_owned(),
        body_html,
        body_text,
        ..PgpMessageStatusDto::default()
    })
}

/// Builds the status for an encrypted message: try each own key, decrypt, then
/// verify any inner signature and render.
async fn decrypt_status(
    store: &Store,
    secrets: &SharedSecretStore,
    ciphertext: &str,
    passphrase: Option<&str>,
) -> Result<PgpMessageStatusDto, MailError> {
    let encrypted_base = PgpMessageStatusDto {
        is_pgp: true,
        encrypted: true,
        signature_status: "none".to_owned(),
        ..PgpMessageStatusDto::default()
    };

    let mut needs_passphrase = false;
    for key in store.pgp_keys().list_own().await? {
        // A passphrase-protected key cannot be tried without one.
        if key.has_passphrase && passphrase.is_none() {
            needs_passphrase = true;
            continue;
        }
        let Some(secret) = read_secret(secrets, credential_ref_for_pgp(&key.fingerprint)).await?
        else {
            continue;
        };
        match decrypt_message(&secret, passphrase, ciphertext) {
            Ok(decrypted) => {
                return decrypted_status(store, &decrypted.bytes, encrypted_base).await;
            }
            Err(_) => continue,
        }
    }

    Ok(PgpMessageStatusDto {
        needs_passphrase,
        error: (!needs_passphrase)
            .then(|| "this message could not be decrypted with any available key".to_owned()),
        ..encrypted_base
    })
}

/// Fills in a decrypted message's rendered body and any inner signature.
async fn decrypted_status(
    store: &Store,
    plaintext: &[u8],
    base: PgpMessageStatusDto,
) -> Result<PgpMessageStatusDto, MailError> {
    // A signed-then-encrypted message reveals a multipart/signed inner entity.
    if let Some(PgpMimeMessage::SignedMime {
        signed_entity,
        signature,
    }) = parse_pgp_mime(plaintext)
    {
        let candidates = candidate_public_keys(store).await?;
        let verification = verify_detached(&candidates, &signature, &signed_entity)?;
        let (body_html, body_text) = render_entity(&signed_entity);
        return Ok(PgpMessageStatusDto {
            signed: true,
            signature_status: status_label(verification).to_owned(),
            decrypted: true,
            body_html,
            body_text,
            ..base
        });
    }
    let (body_html, body_text) = render_entity(plaintext);
    Ok(PgpMessageStatusDto {
        decrypted: true,
        body_html,
        body_text,
        ..base
    })
}

/// Collects all public keys we hold (own + recipient) as verification
/// candidates.
async fn candidate_public_keys(store: &Store) -> Result<Vec<String>, MailError> {
    let mut keys: Vec<String> = store
        .pgp_keys()
        .list_own()
        .await?
        .into_iter()
        .map(|key| key.public_armored)
        .collect();
    keys.extend(
        store
            .pgp_keys()
            .list_recipients()
            .await?
            .into_iter()
            .map(|key| key.public_armored),
    );
    Ok(keys)
}

/// Parses a MIME entity and returns its sanitized HTML and/or plain-text body.
fn render_entity(bytes: &[u8]) -> (Option<String>, Option<String>) {
    match parse_message(bytes) {
        Ok(parsed) => {
            let html = parsed
                .html_body
                .as_deref()
                .map(|html| sanitize_message_html(html).html);
            (html, parsed.text_body)
        }
        Err(_) => (None, Some(String::from_utf8_lossy(bytes).into_owned())),
    }
}

/// Maps a verification result to its wire label.
fn status_label(verification: SignatureVerification) -> &'static str {
    match verification {
        SignatureVerification::Valid => "valid",
        SignatureVerification::Invalid => "invalid",
        SignatureVerification::UnknownKey => "unknownKey",
    }
}
