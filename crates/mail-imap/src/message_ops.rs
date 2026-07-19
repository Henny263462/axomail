//! Single-message server operations: fetch body, flag store, move, delete.
//!
//! These all act on the currently selected mailbox (the caller guarantees
//! it was selected by a preceding `sync`). Each network round-trip is
//! bounded by the configured timeout.

use std::time::Duration;

use axomail_core::{FlagDelta, MailError};
use futures::TryStreamExt;

use crate::connection::Connected;
use crate::convert::flag_set_to_imap_list;
use crate::error::{map_imap_error, with_deadline};

/// Selects `encoded_name` (already modified UTF-7 encoded) and returns its
/// `UIDVALIDITY`.
///
/// Used by the write-back queue to target a mailbox before replaying its
/// per-message ops, independently of a full sync pass.
pub(crate) async fn select(
    connected: &mut Connected,
    encoded_name: &str,
    timeout: Duration,
) -> Result<u32, MailError> {
    with_deadline(timeout, async {
        let mailbox = connected
            .session
            .select(encoded_name)
            .await
            .map_err(|err| map_imap_error(&err))?;
        mailbox.uid_validity.ok_or_else(|| MailError::Protocol {
            message: "the server did not report UIDVALIDITY on SELECT".to_owned(),
        })
    })
    .await
}

/// Fetches the complete raw bytes of one message.
///
/// Uses `BODY.PEEK[]` rather than `BODY[]` so that reading a message never
/// implicitly sets `\Seen`; the seen state is changed only through an
/// explicit `set_flags`.
pub(crate) async fn fetch_body(
    connected: &mut Connected,
    uid: u32,
    timeout: Duration,
) -> Result<Vec<u8>, MailError> {
    with_deadline(timeout, async {
        let stream = connected
            .session
            .uid_fetch(uid.to_string(), "BODY.PEEK[]")
            .await
            .map_err(|err| map_imap_error(&err))?;
        let fetches = stream
            .try_collect::<Vec<_>>()
            .await
            .map_err(|err| map_imap_error(&err))?;
        fetches
            .iter()
            .find_map(|fetch| fetch.body().map(<[u8]>::to_vec))
            .ok_or_else(|| MailError::Protocol {
                message: "the server returned no body for the message".to_owned(),
            })
    })
    .await
}

/// Applies an additive flag change with `UID STORE ... +FLAGS`/`-FLAGS`.
///
/// The added and removed sides are issued as separate `STORE` commands
/// (plain IMAP, capability-agnostic); an empty side is skipped, and an
/// entirely empty delta performs no network round-trip. Because only the
/// named flags are touched, this never clobbers flags or keywords the caller
/// did not mention — the invariant the write-back queue relies on.
pub(crate) async fn store_flags(
    connected: &mut Connected,
    uid: u32,
    delta: &FlagDelta,
    timeout: Duration,
) -> Result<(), MailError> {
    if !delta.add.is_empty() {
        let query = format!("+FLAGS {}", flag_set_to_imap_list(&delta.add));
        run_store(connected, uid, &query, timeout).await?;
    }
    if !delta.remove.is_empty() {
        let query = format!("-FLAGS {}", flag_set_to_imap_list(&delta.remove));
        run_store(connected, uid, &query, timeout).await?;
    }
    Ok(())
}

/// Moves a message to `encoded_target` (already modified UTF-7 encoded).
///
/// Uses the atomic `UID MOVE` when the server advertises `MOVE`; otherwise
/// falls back to the RFC 3501 `UID COPY` + mark `\Deleted` + expunge sequence.
///
/// The fallback is made safe on two counts. **Idempotency**: it first checks
/// the message is still in the source mailbox, so a retry after a completed
/// move (the source UID already gone) does not re-`COPY` it and duplicate it in
/// the target. **No collateral loss**: the expunge is scoped to this UID via
/// UIDPLUS, and without UIDPLUS it is skipped entirely rather than issuing a
/// mailbox-wide `EXPUNGE` (see [`expunge_scoped`]).
///
/// One residual window remains at-least-once: if a prior attempt sent the
/// `COPY` but died before the `\Deleted` store, the source still holds the UID
/// and a retry re-copies it. Callers tolerating exactly-once must prefer a
/// `MOVE`-capable server.
pub(crate) async fn move_to(
    connected: &mut Connected,
    uid: u32,
    encoded_target: &str,
    timeout: Duration,
) -> Result<(), MailError> {
    if connected.has_capability("MOVE") {
        return with_deadline(timeout, async {
            connected
                .session
                .uid_mv(uid.to_string(), encoded_target)
                .await
                .map_err(|err| map_imap_error(&err))
        })
        .await;
    }

    // Idempotency: a message absent from the source was already moved by an
    // earlier attempt. Re-copying would duplicate it in the target.
    if !uid_present(connected, uid, timeout).await? {
        return Ok(());
    }
    with_deadline(timeout, async {
        connected
            .session
            .uid_copy(uid.to_string(), encoded_target)
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await?;
    run_store(connected, uid, "+FLAGS (\\Deleted)", timeout).await?;
    expunge_scoped(connected, uid, timeout).await
}

/// Marks a message `\Deleted` and expunges it, scoped so a server without
/// UIDPLUS never triggers a mailbox-wide purge (see [`expunge_scoped`]).
pub(crate) async fn delete(
    connected: &mut Connected,
    uid: u32,
    timeout: Duration,
) -> Result<(), MailError> {
    run_store(connected, uid, "+FLAGS (\\Deleted)", timeout).await?;
    expunge_scoped(connected, uid, timeout).await
}

/// Whether the currently selected mailbox still holds `uid`
/// (`UID SEARCH UID <uid>`). Used by the COPY fallback to avoid re-copying a
/// message a prior attempt already moved.
async fn uid_present(
    connected: &mut Connected,
    uid: u32,
    timeout: Duration,
) -> Result<bool, MailError> {
    let found = with_deadline(timeout, async {
        connected
            .session
            .uid_search(format!("UID {uid}"))
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await?;
    Ok(found.contains(&uid))
}

/// Runs a `UID STORE` with the given flag query, draining the response.
async fn run_store(
    connected: &mut Connected,
    uid: u32,
    query: &str,
    timeout: Duration,
) -> Result<(), MailError> {
    with_deadline(timeout, async {
        let stream = connected
            .session
            .uid_store(uid.to_string(), query)
            .await
            .map_err(|err| map_imap_error(&err))?;
        stream
            .try_collect::<Vec<_>>()
            .await
            .map_err(|err| map_imap_error(&err))?;
        Ok(())
    })
    .await
}

/// Expunges exactly one message when it is safe to do so.
///
/// Under `UIDPLUS`, `UID EXPUNGE <uid>` removes precisely this UID. Without
/// `UIDPLUS`, a plain `EXPUNGE` would remove **every** `\Deleted` message in
/// the mailbox — permanently deleting unrelated messages the user (or another
/// op) flagged `\Deleted` — so it is deliberately **not** issued. The message
/// stays flagged `\Deleted` in the source and is reclaimed by a later
/// expunge/compaction (e.g. an eventual `MOVE`-capable pass or a mailbox
/// close): leaving a tombstone is safe, silently deleting unrelated mail is not.
async fn expunge_scoped(
    connected: &mut Connected,
    uid: u32,
    timeout: Duration,
) -> Result<(), MailError> {
    if !connected.has_capability("UIDPLUS") {
        // No scoped expunge available: skip rather than purge the whole mailbox.
        return Ok(());
    }
    with_deadline(timeout, async {
        let stream = connected
            .session
            .uid_expunge(uid.to_string())
            .await
            .map_err(|err| map_imap_error(&err))?;
        stream
            .try_collect::<Vec<_>>()
            .await
            .map_err(|err| map_imap_error(&err))?;
        Ok(())
    })
    .await
}
