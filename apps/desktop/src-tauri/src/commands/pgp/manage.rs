//! Key-management logic for the PGP commands: generate / import / export /
//! list / delete own keys, and add / list / delete recipient keys.
//!
//! Secret key material is written to and read from the OS keychain here (via
//! blocking tasks, ADR-0001); the database only ever sees the public half and
//! metadata. The heavier CPU work (key generation, parsing) runs on a blocking
//! task so the async runtime is never stalled.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axomail_core::{CredentialRef, MailError};
use axomail_security::pgp::{
    self, credential_ref_for_pgp, import_public_key, import_secret_key, public_from_secret,
};
use axomail_storage::{NewPgpOwnKey, NewPgpRecipientKey, PgpOwnKeyId, PgpRecipientKeyId, Store};

use crate::dto::{PgpKeyDto, PgpRecipientKeyDto};
use crate::state::SharedSecretStore;

/// Current time in Unix seconds.
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Writes a secret to the keychain on a blocking task.
async fn store_secret(
    secrets: &SharedSecretStore,
    reference: CredentialRef,
    secret: String,
) -> Result<(), MailError> {
    let secrets = Arc::clone(secrets);
    match tokio::task::spawn_blocking(move || secrets.set_secret(&reference, &secret)).await {
        Ok(result) => result,
        Err(_) => Err(MailError::Keychain {
            message: "keychain write did not complete".to_owned(),
        }),
    }
}

/// Reads a secret from the keychain on a blocking task.
pub(super) async fn read_secret(
    secrets: &SharedSecretStore,
    reference: CredentialRef,
) -> Result<Option<String>, MailError> {
    let secrets = Arc::clone(secrets);
    match tokio::task::spawn_blocking(move || secrets.get_secret(&reference)).await {
        Ok(result) => result,
        Err(_) => Err(MailError::Keychain {
            message: "keychain read did not complete".to_owned(),
        }),
    }
}

/// Deletes a secret from the keychain on a blocking task.
async fn delete_secret(
    secrets: &SharedSecretStore,
    reference: CredentialRef,
) -> Result<(), MailError> {
    let secrets = Arc::clone(secrets);
    match tokio::task::spawn_blocking(move || secrets.delete_secret(&reference)).await {
        Ok(result) => result,
        Err(_) => Err(MailError::Keychain {
            message: "keychain delete did not complete".to_owned(),
        }),
    }
}

/// Generates a new keypair, stores its secret in the keychain, and records the
/// public metadata.
pub async fn generate_key_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    user_id: String,
    passphrase: Option<String>,
) -> Result<PgpKeyDto, MailError> {
    let generated =
        tokio::task::spawn_blocking(move || pgp::generate_keypair(&user_id, passphrase.as_deref()))
            .await
            .map_err(|_| MailError::Crypto {
                message: "key generation did not complete".to_owned(),
            })??;

    let fingerprint = generated.metadata.fingerprint.clone();
    store_secret(
        secrets,
        credential_ref_for_pgp(&fingerprint),
        generated.secret_armored,
    )
    .await?;
    let key = store
        .pgp_keys()
        .insert_own(&own_from_metadata(
            &generated.metadata,
            generated.public_armored,
        ))
        .await?;
    PgpKeyDto::from_own(key)
}

/// Imports an existing armored secret key: stores its secret in the keychain and
/// records its public metadata.
pub(super) async fn import_secret_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    armored: String,
) -> Result<PgpKeyDto, MailError> {
    let (secret_armored, metadata) = import_secret_key(&armored)?;
    let (public_armored, _) = public_from_secret(&secret_armored)?;
    store_secret(
        secrets,
        credential_ref_for_pgp(&metadata.fingerprint),
        secret_armored,
    )
    .await?;
    let key = store
        .pgp_keys()
        .insert_own(&own_from_metadata(&metadata, public_armored))
        .await?;
    PgpKeyDto::from_own(key)
}

/// Imports a recipient's armored public key for `email`.
pub async fn import_recipient_impl(
    store: &Store,
    email: String,
    armored: String,
) -> Result<PgpRecipientKeyDto, MailError> {
    let (public_armored, metadata) = import_public_key(&armored)?;
    let now = now_unix();
    let key = store
        .pgp_keys()
        .upsert_recipient(&NewPgpRecipientKey {
            email,
            fingerprint: metadata.fingerprint,
            key_id: metadata.key_id,
            user_id: metadata.user_ids.first().cloned().unwrap_or_default(),
            algorithm: metadata.algorithm,
            public_armored,
            created_at_unix: metadata.created_at_unix,
            added_at_unix: now,
        })
        .await?;
    PgpRecipientKeyDto::from_recipient(key)
}

/// Lists all own keys.
pub async fn list_own_impl(store: &Store) -> Result<Vec<PgpKeyDto>, MailError> {
    store
        .pgp_keys()
        .list_own()
        .await?
        .into_iter()
        .map(PgpKeyDto::from_own)
        .collect()
}

/// Lists all recipient keys.
pub(super) async fn list_recipients_impl(
    store: &Store,
) -> Result<Vec<PgpRecipientKeyDto>, MailError> {
    store
        .pgp_keys()
        .list_recipients()
        .await?
        .into_iter()
        .map(PgpRecipientKeyDto::from_recipient)
        .collect()
}

/// Returns the armored public key for an own key (for sharing / backup).
pub(super) async fn export_public_impl(store: &Store, id: i64) -> Result<String, MailError> {
    store
        .pgp_keys()
        .get_own(PgpOwnKeyId::new(id))
        .await?
        .map(|key| key.public_armored)
        .ok_or_else(|| MailError::Configuration {
            message: "the requested PGP key does not exist".to_owned(),
        })
}

/// Returns the armored secret key for an own key (an explicit, user-initiated
/// backup). The secret is read from the keychain; the caller — the settings
/// key manager — presents it for the user to save.
pub(super) async fn export_secret_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    id: i64,
) -> Result<String, MailError> {
    let key = store
        .pgp_keys()
        .get_own(PgpOwnKeyId::new(id))
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the requested PGP key does not exist".to_owned(),
        })?;
    read_secret(secrets, credential_ref_for_pgp(&key.fingerprint))
        .await?
        .ok_or_else(|| MailError::Keychain {
            message: "no secret key is stored for this fingerprint".to_owned(),
        })
}

/// Makes an own key the default signing key.
pub(super) async fn set_default_impl(store: &Store, id: i64) -> Result<(), MailError> {
    store.pgp_keys().set_default_own(PgpOwnKeyId::new(id)).await
}

/// Deletes an own key and purges its keychain secret.
pub(super) async fn delete_own_impl(
    store: &Store,
    secrets: &SharedSecretStore,
    id: i64,
) -> Result<bool, MailError> {
    let Some(removed) = store.pgp_keys().delete_own(PgpOwnKeyId::new(id)).await? else {
        return Ok(false);
    };
    // Best-effort: a failed keychain delete leaves an orphan secret but must not
    // fail the row deletion the user already saw succeed.
    if let Err(error) = delete_secret(secrets, credential_ref_for_pgp(&removed.fingerprint)).await {
        tracing::warn!(error = %error, "could not delete PGP secret from keychain");
    }
    Ok(true)
}

/// Deletes a recipient key.
pub(super) async fn delete_recipient_impl(store: &Store, id: i64) -> Result<bool, MailError> {
    store
        .pgp_keys()
        .delete_recipient(PgpRecipientKeyId::new(id))
        .await
}

/// Builds the storage insert input from generated/imported key metadata.
fn own_from_metadata(metadata: &pgp::KeyMetadata, public_armored: String) -> NewPgpOwnKey {
    NewPgpOwnKey {
        fingerprint: metadata.fingerprint.clone(),
        key_id: metadata.key_id.clone(),
        user_id: metadata.user_ids.first().cloned().unwrap_or_default(),
        algorithm: metadata.algorithm.clone(),
        public_armored,
        has_passphrase: metadata.is_passphrase_protected,
        created_at_unix: metadata.created_at_unix,
        added_at_unix: now_unix(),
    }
}
