//! Read-side PGP logic: the composer's sign/encrypt availability, and the
//! reading pane's verify-and-decrypt analysis of a received message.

use std::path::Path;

use axomail_core::{IncomingMailProvider, MailError};
use axomail_parser::parse_message;
use axomail_security::pgp::{
    PgpMimeMessage, SignatureVerification, credential_ref_for_pgp, decrypt_message, parse_pgp_mime,
    public_key_can_encrypt, verify_detached,
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
        .own_key_for_address(&normalize_email(&from_email))
        .await?
        .is_some();

    let mut missing = Vec::new();
    for recipient in &recipients {
        let email = normalize_email(recipient);
        if email.is_empty() {
            continue;
        }
        // Encryption is only actually possible when a held key for this recipient
        // has a *usable* encryption (sub)key (finding #8): a recipient key first,
        // else one of our own keys for a self-address. A key present but with no
        // valid (non-revoked/expired) encryption subkey does not count — offering
        // Encrypt there would only make the send fail.
        let recipient_key = store.pgp_keys().recipient_public_for(&email).await?;
        let own_key = store
            .pgp_keys()
            .own_key_for_address(&email)
            .await?
            .map(|key| key.public_armored);
        let can_encrypt_to = recipient_key
            .into_iter()
            .chain(own_key)
            .any(|armored| public_key_can_encrypt(&armored));
        if !can_encrypt_to {
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

    // The claimed sender: a signature is only "valid" when the signing key's
    // identity matches this address (finding #1). Taken from the outer message's
    // From, which is the identity the reader sees.
    let from_email = sender_email(&raw);

    match parsed {
        PgpMimeMessage::SignedMime {
            signed_entity,
            signature,
        } => verified_status(store, &signed_entity, &signature, &from_email).await,
        PgpMimeMessage::EncryptedMime { ciphertext }
        | PgpMimeMessage::InlineEncrypted { ciphertext } => {
            decrypt_status(
                store,
                secrets,
                &ciphertext,
                passphrase.as_deref(),
                &from_email,
            )
            .await
        }
    }
}

/// The sender email from a raw message's `From`, or the empty string. Used to
/// bind signature verification to the claimed sender.
fn sender_email(raw: &[u8]) -> String {
    parse_message(raw)
        .ok()
        .and_then(|parsed| {
            parsed
                .from
                .first()
                .map(|address| address.email.as_str().to_owned())
        })
        .unwrap_or_default()
}

/// Builds the status for a signed (unencrypted) message: verify against the
/// sender and render.
async fn verified_status(
    store: &Store,
    signed_entity: &[u8],
    signature: &str,
    from_email: &str,
) -> Result<PgpMessageStatusDto, MailError> {
    let candidates = candidate_public_keys(store).await?;
    let verified = verify_detached(&candidates, signature, signed_entity, from_email)?;
    let (body_html, body_text) = render_entity(signed_entity);
    Ok(PgpMessageStatusDto {
        is_pgp: true,
        signed: true,
        signature_status: status_label(verified.verification).to_owned(),
        signer: verified.signer,
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
    from_email: &str,
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
                return decrypted_status(store, &decrypted.bytes, from_email, encrypted_base).await;
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
    from_email: &str,
    base: PgpMessageStatusDto,
) -> Result<PgpMessageStatusDto, MailError> {
    // A signed-then-encrypted message reveals a multipart/signed inner entity.
    if let Some(PgpMimeMessage::SignedMime {
        signed_entity,
        signature,
    }) = parse_pgp_mime(plaintext)
    {
        let candidates = candidate_public_keys(store).await?;
        let verified = verify_detached(&candidates, &signature, &signed_entity, from_email)?;
        let (body_html, body_text) = render_entity(&signed_entity);
        return Ok(PgpMessageStatusDto {
            signed: true,
            signature_status: status_label(verified.verification).to_owned(),
            signer: verified.signer,
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

/// Maps a verification result to its wire label. `signerMismatch` is a distinct
/// status from `valid`: the signature is cryptographically sound but the signing
/// key's identity does not match the sender, so the badge must not read green.
fn status_label(verification: SignatureVerification) -> &'static str {
    match verification {
        SignatureVerification::Valid => "valid",
        SignatureVerification::SignerMismatch => "signerMismatch",
        SignatureVerification::Invalid => "invalid",
        SignatureVerification::UnknownKey => "unknownKey",
    }
}
