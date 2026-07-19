//! Authoritative present-set reconciliation for IMAP sync passes.
//!
//! IMAP cannot cheaply enumerate the UIDs that vanished, so the adapter
//! instead reports the *complete* set of UIDs still present. This module diffs
//! that set against the mailbox's stored rows (bounded by the local row count,
//! never by the sparse UID space) and deletes the ones the server no longer
//! lists — inside the same ingest transaction that applied the pass.

use std::collections::HashSet;

use axomail_core::{MailError, MailboxId, RemoteMessageId};

use crate::error_map::map_sqlx;

use super::delete_message;

/// Deletes stored IMAP messages absent from the server's present set.
///
/// Loads the mailbox's stored `(uid, uid_validity)` pairs, diffs them in
/// memory against the reported present ids, and deletes the absent ones inside
/// the ingest transaction. Returns how many rows were removed.
pub(super) async fn delete_absent(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    mailbox: MailboxId,
    present: &[RemoteMessageId],
) -> Result<u64, MailError> {
    let present_set: HashSet<(u32, u32)> = present
        .iter()
        .filter_map(|id| match id {
            RemoteMessageId::Imap { uid, uid_validity } => Some((*uid, *uid_validity)),
            RemoteMessageId::Pop3 { .. } => None,
        })
        .collect();

    let stored: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT remote_uid, remote_uid_validity FROM messages
         WHERE mailbox_id = ? AND remote_kind = 'imap'
           AND remote_uid IS NOT NULL AND remote_uid_validity IS NOT NULL",
    )
    .bind(mailbox.value())
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| map_sqlx("ingest_present_load", error))?;

    let mut removed = 0u64;
    for (uid, validity) in stored {
        let (Ok(uid), Ok(validity)) = (u32::try_from(uid), u32::try_from(validity)) else {
            continue;
        };
        if !present_set.contains(&(uid, validity)) {
            let remote_id = RemoteMessageId::Imap {
                uid,
                uid_validity: validity,
            };
            removed += delete_message(tx, mailbox, &remote_id).await?;
        }
    }
    Ok(removed)
}
