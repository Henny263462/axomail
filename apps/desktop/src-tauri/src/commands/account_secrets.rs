//! Blocking keychain and raw-cache side effects for the account commands.
//!
//! Keychain access blocks, so every secret operation runs on
//! `spawn_blocking`. Deletions (of secrets and of cached raw bodies) are
//! best-effort: they log and swallow failures so a locked or already-missing
//! resource can never fail an account create/delete.

use std::path::Path;
use std::sync::Arc;

use axomail_core::{CredentialRef, MailError};

use crate::commands::detail::raw_message_path;
use crate::state::SharedSecretStore;

/// Stores a secret on a blocking task (keychain I/O blocks).
pub(crate) async fn set_secret(
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

/// Deletes a secret on a blocking task, logging but swallowing any failure.
pub(crate) async fn delete_secret_best_effort(
    secrets: &SharedSecretStore,
    reference: &CredentialRef,
) {
    let secrets = Arc::clone(secrets);
    let reference = reference.clone();
    match tokio::task::spawn_blocking(move || secrets.delete_secret(&reference)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(error = %error, "failed to delete account secret from keychain");
        }
        Err(_) => tracing::warn!("keychain deletion task did not complete"),
    }
}

/// Best-effort removal of a message's cached raw body. A missing file is a
/// normal outcome (the body was never fetched); any other error is logged and
/// swallowed so it cannot fail account deletion.
pub(crate) async fn remove_raw_best_effort(raw_root: &Path, message_id: i64) {
    let path = raw_message_path(raw_root, message_id);
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(
                message_id,
                kind = ?error.kind(),
                "failed to delete cached raw message body on account removal"
            );
        }
    }
}
