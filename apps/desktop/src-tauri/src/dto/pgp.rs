//! OpenPGP DTOs (Track 5.1): own/recipient key descriptions, the composer's
//! recipient-key status, and a received message's PGP security status.
//!
//! Every id crosses the IPC boundary through [`checked_id`]. No secret key
//! material or passphrase is ever placed in a DTO — an own key exposes only its
//! public metadata (ADR-0001/0003).

use axomail_core::MailError;
use axomail_storage::{PgpOwnKey, PgpRecipientKey};
use serde::Serialize;

use super::checked_id;

/// One of our own OpenPGP keys, as listed in the key manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PgpKeyDto {
    /// Local identifier.
    pub id: i64,
    /// Full fingerprint, lowercase hex.
    pub fingerprint: String,
    /// Long key id, lowercase hex.
    pub key_id: String,
    /// Primary user id string.
    pub user_id: String,
    /// Public-key algorithm of the primary key.
    pub algorithm: String,
    /// Whether the keychain-held secret is passphrase-protected.
    pub has_passphrase: bool,
    /// Whether this is the default signing key.
    pub is_default: bool,
    /// Key creation time (Unix seconds).
    pub created_at_unix: i64,
}

impl PgpKeyDto {
    /// Builds the DTO from a stored own key, validating its id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the id is out of the safe range.
    pub fn from_own(key: PgpOwnKey) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(key.id.value())?,
            fingerprint: key.fingerprint,
            key_id: key.key_id,
            user_id: key.user_id,
            algorithm: key.algorithm,
            has_passphrase: key.has_passphrase,
            is_default: key.is_default,
            created_at_unix: key.created_at_unix,
        })
    }
}

/// A recipient (contact) public key, as listed in the key manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PgpRecipientKeyDto {
    /// Local identifier.
    pub id: i64,
    /// Lowercased recipient email this key encrypts to.
    pub email: String,
    /// Full fingerprint, lowercase hex.
    pub fingerprint: String,
    /// Long key id, lowercase hex.
    pub key_id: String,
    /// Primary user id string.
    pub user_id: String,
    /// Public-key algorithm of the primary key.
    pub algorithm: String,
    /// Key creation time (Unix seconds).
    pub created_at_unix: i64,
}

impl PgpRecipientKeyDto {
    /// Builds the DTO from a stored recipient key, validating its id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the id is out of the safe range.
    pub fn from_recipient(key: PgpRecipientKey) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(key.id.value())?,
            email: key.email,
            fingerprint: key.fingerprint,
            key_id: key.key_id,
            user_id: key.user_id,
            algorithm: key.algorithm,
            created_at_unix: key.created_at_unix,
        })
    }
}

/// Whether an outgoing message can be signed and/or encrypted, for the composer
/// to enable or dim its toggles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PgpComposeStatusDto {
    /// True when at least one own key exists (signing is possible).
    pub can_sign: bool,
    /// True when a public key is held for **every** recipient (encryption is
    /// possible).
    pub can_encrypt: bool,
    /// Recipient addresses with no known public key (why encryption is off).
    pub missing_recipients: Vec<String>,
}

/// The OpenPGP security status of a received message, for the reading pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PgpMessageStatusDto {
    /// Whether the message carries any OpenPGP payload at all.
    pub is_pgp: bool,
    /// Whether the message is encrypted.
    pub encrypted: bool,
    /// Whether the message is signed.
    pub signed: bool,
    /// Signature outcome: `none`, `valid`, `invalid`, or `unknownKey`.
    pub signature_status: String,
    /// The matched signer's user id, when a signing key was found.
    pub signer: Option<String>,
    /// Whether an encrypted message was successfully decrypted.
    pub decrypted: bool,
    /// Whether decryption needs a passphrase that was not (yet) supplied.
    pub needs_passphrase: bool,
    /// Sanitized decrypted HTML body (ADR-0007), when available.
    pub body_html: Option<String>,
    /// Decrypted plain-text body, when available.
    pub body_text: Option<String>,
    /// Redacted failure reason, if the operation could not complete.
    pub error: Option<String>,
}

impl PgpMessageStatusDto {
    /// A "this message is not OpenPGP" status.
    #[must_use]
    pub fn not_pgp() -> Self {
        Self {
            signature_status: "none".to_owned(),
            ..Self::default()
        }
    }
}
