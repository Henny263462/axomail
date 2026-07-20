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
/// recipient public keys to encrypt to (all empty ⇒ sign only).
#[derive(Debug, Clone)]
pub(crate) struct PgpSendPlan {
    /// Signing key material, when signing was requested.
    pub signer: Option<PgpSigner>,
    /// Armored public keys of the *visible* recipients (To/Cc) plus ourselves.
    pub encrypt_to: Vec<String>,
    /// Armored public keys of the *hidden* (Bcc) recipients. These are encrypted
    /// to with a wildcard key id so the ciphertext never discloses a Bcc'd
    /// party.
    pub encrypt_to_hidden: Vec<String>,
}

/// Resolves the sign/encrypt plan for a message from `from_email` to its
/// `visible_recipients` (To/Cc) and `bcc_recipients` (Bcc), or `None` when
/// neither protection is requested.
///
/// Signing requires a key whose User-ID matches `from_email` — there is **no**
/// fallback to a default key, so a message is never signed (or encrypted-to-self)
/// under an identity other than the one it is sent as. Bcc recipients are
/// carried separately from To/Cc so encryption can hide them.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] when signing is requested but no signing
/// key matches the sender, or encryption is requested but a recipient has no
/// known public key; and [`MailError::Keychain`] if the signer's secret cannot
/// be read.
// Each argument is a distinct resource or independent flag (store, secrets,
// sender, the two recipient sets, sign/encrypt, passphrase); grouping them into
// a struct would not add clarity, so they are passed individually.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve_send_plan(
    store: &Store,
    secrets: &SharedSecretStore,
    from_email: &str,
    visible_recipients: &[String],
    bcc_recipients: &[String],
    sign: bool,
    encrypt: bool,
    passphrase: Option<String>,
) -> Result<Option<PgpSendPlan>, MailError> {
    if !sign && !encrypt {
        return Ok(None);
    }

    let from = normalize_email(from_email);
    // Strict: the signing/self key must actually belong to the sending From —
    // never the unrelated default key.
    let own = store.pgp_keys().own_key_for_address(&from).await?;

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

    let (encrypt_to, encrypt_to_hidden) = if encrypt {
        let mut visible = resolve_recipient_keys(store, visible_recipients).await?;
        let hidden = resolve_recipient_keys(store, bcc_recipients).await?;
        // Encrypt to ourselves too (as a visible recipient — it is our own key),
        // so the filed Sent copy stays readable.
        if let Some(own) = own {
            visible.push(own.public_armored);
        }
        (visible, hidden)
    } else {
        (Vec::new(), Vec::new())
    };

    Ok(Some(PgpSendPlan {
        signer,
        encrypt_to,
        encrypt_to_hidden,
    }))
}

/// Resolves the armored public key for each recipient address, failing if any
/// recipient has no known key. A recipient key takes precedence; an own key for
/// the address is accepted as a fallback (encrypting to a self-address).
async fn resolve_recipient_keys(
    store: &Store,
    recipients: &[String],
) -> Result<Vec<String>, MailError> {
    let mut keys = Vec::with_capacity(recipients.len());
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
    Ok(keys)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::sync::Arc;

    use axomail_security::InMemorySecretStore;
    use axomail_storage::{NewPgpOwnKey, Store};

    use super::*;

    fn own_key(user_id: &str) -> NewPgpOwnKey {
        NewPgpOwnKey {
            fingerprint: "aaaa000000000000000000000000000000000001".to_owned(),
            key_id: "0000000000000001".to_owned(),
            user_id: user_id.to_owned(),
            algorithm: "EdDSALegacy".to_owned(),
            public_armored: "unused-in-this-test".to_owned(),
            has_passphrase: false,
            created_at_unix: 0,
            added_at_unix: 0,
        }
    }

    #[tokio::test]
    async fn signing_without_a_matching_key_errors_instead_of_using_the_default() {
        // SECURITY (finding #5): the only (default) key belongs to personal@;
        // sending as alias@ (no matching key) must FAIL rather than silently sign
        // under the default personal@ key.
        let store = Store::open_in_memory().await.unwrap();
        store
            .pgp_keys()
            .insert_own(&own_key("Personal <personal@example.org>"))
            .await
            .unwrap();
        let secrets: SharedSecretStore = Arc::new(InMemorySecretStore::new());

        let result = resolve_send_plan(
            &store,
            &secrets,
            "alias@example.org",
            &["bob@example.org".to_owned()],
            &[],
            true,  // sign
            false, // encrypt
            None,
        )
        .await;

        assert!(
            matches!(result, Err(MailError::Configuration { .. })),
            "signing as an address with no matching key must error, got {result:?}"
        );
    }
}
