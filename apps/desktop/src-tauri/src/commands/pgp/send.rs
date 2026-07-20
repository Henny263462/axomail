//! Resolving the sign/encrypt plan for an outgoing message.
//!
//! The composer requests signing and/or encryption; this turns that request
//! into the concrete key material the send pipeline applies (RFC 3156
//! PGP/MIME). Resolution fails the send up front if a required key is missing —
//! no signing key for the sender, or a recipient without a public key — so a
//! message is never sent unprotected when protection was asked for. The signer's
//! passphrase rides only on this in-memory plan and is never persisted.

use axomail_core::MailError;
use axomail_storage::Store;

use super::manage::read_secret;
use super::normalize_email;
use crate::state::SharedSecretStore;
use axomail_security::pgp::credential_ref_for_pgp;

/// The signing key material for an outgoing message. Its [`Debug`] redacts both
/// the armored secret and the passphrase so a stray `{:?}` cannot leak them.
#[derive(Clone)]
pub(crate) struct PgpSigner {
    /// Armored secret key resolved from the keychain.
    pub secret_armored: String,
    /// User-entered passphrase, if the key is protected. Never stored.
    pub passphrase: Option<String>,
}

impl std::fmt::Debug for PgpSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgpSigner")
            .field("secret_armored", &"<redacted>")
            .field(
                "passphrase",
                &self.passphrase.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// A resolved PGP protection plan for one send: optional signer plus the
/// recipient public keys to encrypt to (empty ⇒ sign only).
#[derive(Debug, Clone)]
pub(crate) struct PgpSendPlan {
    /// Signing key material, when signing was requested.
    pub signer: Option<PgpSigner>,
    /// Armored recipient public keys, when encryption was requested.
    pub encrypt_to: Vec<String>,
}

/// Resolves the sign/encrypt plan for a message from `from_email` to
/// `recipients`, or `None` when neither protection is requested.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] when signing is requested but no signing
/// key matches the sender, or encryption is requested but a recipient has no
/// known public key; and [`MailError::Keychain`] if the signer's secret cannot
/// be read.
pub(crate) async fn resolve_send_plan(
    store: &Store,
    secrets: &SharedSecretStore,
    from_email: &str,
    recipients: &[String],
    sign: bool,
    encrypt: bool,
    passphrase: Option<String>,
) -> Result<Option<PgpSendPlan>, MailError> {
    if !sign && !encrypt {
        return Ok(None);
    }

    let from = normalize_email(from_email);
    let own = store.pgp_keys().own_for_email(&from).await?;

    let signer = if sign {
        let key = own.clone().ok_or_else(|| MailError::Configuration {
            message: "no OpenPGP signing key is configured for the sender".to_owned(),
        })?;
        let secret = read_secret(secrets, credential_ref_for_pgp(&key.fingerprint))
            .await?
            .ok_or_else(|| MailError::Keychain {
                message: "the signing key's secret is missing from the keychain".to_owned(),
            })?;
        Some(PgpSigner {
            secret_armored: secret,
            passphrase,
        })
    } else {
        None
    };

    let encrypt_to = if encrypt {
        let mut keys = Vec::new();
        for recipient in recipients {
            let email = normalize_email(recipient);
            let public = match store.pgp_keys().recipient_public_for(&email).await? {
                Some(public) => public,
                None => store
                    .pgp_keys()
                    .own_key_for_address(&email)
                    .await?
                    .map(|key| key.public_armored)
                    .ok_or_else(|| MailError::Configuration {
                        message: format!("no OpenPGP public key is known for {recipient}"),
                    })?,
            };
            keys.push(public);
        }
        // Encrypt to ourselves too, so the filed Sent copy stays readable.
        if let Some(own) = own {
            keys.push(own.public_armored);
        }
        keys
    } else {
        Vec::new()
    };

    Ok(Some(PgpSendPlan { signer, encrypt_to }))
}
