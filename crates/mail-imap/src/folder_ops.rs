//! Server-side folder management: create, rename, delete, subscribe.
//!
//! Each function acts on a full mailbox path that has already been encoded to
//! modified UTF-7 by the caller (the provider), and bounds its single network
//! round-trip with the configured timeout. These are plain RFC 3501 commands
//! (`CREATE`/`RENAME`/`DELETE`/`SUBSCRIBE`/`UNSUBSCRIBE`) with no capability
//! gate — every IMAP server supports them.

use std::time::Duration;

use axomail_core::MailError;

use crate::connection::Connected;
use crate::error::{map_imap_error, with_deadline};

/// Creates a mailbox at `encoded_path` (already modified UTF-7 encoded).
pub(crate) async fn create(
    connected: &mut Connected,
    encoded_path: &str,
    timeout: Duration,
) -> Result<(), MailError> {
    with_deadline(timeout, async {
        connected
            .session
            .create(encoded_path)
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await
}

/// Renames (and/or moves) a mailbox from `encoded_from` to `encoded_to`.
pub(crate) async fn rename(
    connected: &mut Connected,
    encoded_from: &str,
    encoded_to: &str,
    timeout: Duration,
) -> Result<(), MailError> {
    with_deadline(timeout, async {
        connected
            .session
            .rename(encoded_from, encoded_to)
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await
}

/// Deletes the mailbox at `encoded_path`.
pub(crate) async fn delete(
    connected: &mut Connected,
    encoded_path: &str,
    timeout: Duration,
) -> Result<(), MailError> {
    with_deadline(timeout, async {
        connected
            .session
            .delete(encoded_path)
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await
}

/// Subscribes to (`subscribed = true`) or unsubscribes from the mailbox at
/// `encoded_path`.
pub(crate) async fn set_subscribed(
    connected: &mut Connected,
    encoded_path: &str,
    subscribed: bool,
    timeout: Duration,
) -> Result<(), MailError> {
    with_deadline(timeout, async {
        let session = &mut connected.session;
        let result = if subscribed {
            session.subscribe(encoded_path).await
        } else {
            session.unsubscribe(encoded_path).await
        };
        result.map_err(|err| map_imap_error(&err))
    })
    .await
}
