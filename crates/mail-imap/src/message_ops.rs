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
/// Uses `UID MOVE` when the server advertises `MOVE`; otherwise falls back
/// to `UID COPY` + mark `\Deleted` + expunge.
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

    with_deadline(timeout, async {
        connected
            .session
            .uid_copy(uid.to_string(), encoded_target)
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await?;
    run_store(connected, uid, "+FLAGS (\\Deleted)", timeout).await?;
    expunge(connected, uid, timeout).await
}

/// Marks a message `\Deleted` and expunges it.
pub(crate) async fn delete(
    connected: &mut Connected,
    uid: u32,
    timeout: Duration,
) -> Result<(), MailError> {
    run_store(connected, uid, "+FLAGS (\\Deleted)", timeout).await?;
    expunge(connected, uid, timeout).await
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

/// Expunges one message: `UID EXPUNGE` under `UIDPLUS`, else `EXPUNGE`.
///
/// The `EXPUNGE` fallback removes every `\Deleted` message in the mailbox,
/// not only this UID; this is the documented cost of a server without
/// `UIDPLUS`.
async fn expunge(connected: &mut Connected, uid: u32, timeout: Duration) -> Result<(), MailError> {
    if connected.has_capability("UIDPLUS") {
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
    } else {
        with_deadline(timeout, async {
            let stream = connected
                .session
                .expunge()
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
}
