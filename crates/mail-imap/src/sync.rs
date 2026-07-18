//! One incremental synchronization pass over a mailbox.
//!
//! Selecting the mailbox yields `UIDVALIDITY`. If it differs from the
//! caller's last-known value (or there was none), the full current picture
//! is returned and the engine resets its cache. Otherwise only the delta
//! is computed: new messages, flag changes in a bounded window, and
//! removals.

use std::collections::HashSet;
use std::time::Duration;

use async_imap::types::Mailbox;
use axomail_core::{
    MailError, MailboxSyncState, MessageFlags, RemoteEnvelope, RemoteMessageId, SyncOutcome,
    SyncRequest,
};
use futures::TryStreamExt;

use crate::connection::Connected;
use crate::convert::{envelope_from_fetch, flags_from_fetch, format_uid_set};
use crate::error::{map_imap_error, with_deadline};
use crate::utf7;

/// FETCH items requested for every envelope.
const ENVELOPE_ITEMS: &str = "(ENVELOPE FLAGS RFC822.SIZE INTERNALDATE)";

/// Upper bound on how many of the highest known UIDs are re-checked for
/// flag changes each pass. Without CONDSTORE the whole known range would
/// otherwise be re-fetched; this caps the cost while still catching recent
/// changes. A future CONDSTORE path would replace this heuristic.
const FLAG_WINDOW: u32 = 500;

/// Runs a synchronization pass and returns the outcome to persist.
pub(crate) async fn run(
    connected: &mut Connected,
    request: &SyncRequest,
    timeout: Duration,
) -> Result<SyncOutcome, MailError> {
    tracing::debug!(mailbox = %request.remote_name, "starting IMAP sync pass");
    let encoded = utf7::encode(request.remote_name.as_str());
    let mailbox = select_mailbox(connected, &encoded, timeout).await?;
    let uid_validity = mailbox.uid_validity.ok_or_else(|| MailError::Protocol {
        message: "the server did not report UIDVALIDITY on SELECT".to_owned(),
    })?;

    let batch = request.batch_size.max(1) as usize;
    if request.state.uid_validity == Some(uid_validity) {
        incremental(connected, uid_validity, &request.state, batch, timeout).await
    } else {
        full_picture(connected, uid_validity, batch, timeout).await
    }
}

/// Selects the mailbox, returning its metadata.
async fn select_mailbox(
    connected: &mut Connected,
    encoded_name: &str,
    timeout: Duration,
) -> Result<Mailbox, MailError> {
    with_deadline(timeout, async {
        connected
            .session
            .select(encoded_name)
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await
}

/// Returns the first `batch` messages as a complete snapshot (used on the
/// first sync and whenever `UIDVALIDITY` changed).
async fn full_picture(
    connected: &mut Connected,
    uid_validity: u32,
    batch: usize,
    timeout: Duration,
) -> Result<SyncOutcome, MailError> {
    let all = uid_search_all(connected, timeout).await?;
    let selected: Vec<u32> = all.iter().copied().take(batch).collect();
    let has_more = all.len() > selected.len();

    let new_envelopes = if selected.is_empty() {
        Vec::new()
    } else {
        fetch_envelopes(connected, &format_uid_set(&selected), uid_validity, timeout).await?
    };
    let highest_uid = selected.last().copied();

    Ok(SyncOutcome {
        uid_validity: Some(uid_validity),
        highest_uid,
        has_more,
        new_envelopes,
        flag_updates: Vec::new(),
        removed: Vec::new(),
    })
}

/// Computes the delta since the last pass under an unchanged `UIDVALIDITY`.
async fn incremental(
    connected: &mut Connected,
    uid_validity: u32,
    state: &MailboxSyncState,
    batch: usize,
    timeout: Duration,
) -> Result<SyncOutcome, MailError> {
    let known_high = state.highest_uid.unwrap_or(0);

    // New messages strictly above the known high-water mark. The `N:*`
    // form can re-return the single highest message when N exceeds it, so
    // results are filtered to `uid > known_high`.
    let range = format!("{}:*", known_high.saturating_add(1));
    let mut fetched = fetch_envelopes(connected, &range, uid_validity, timeout).await?;
    fetched.retain(|env| uid_of(env) > known_high);
    fetched.sort_by_key(uid_of);
    let has_more = fetched.len() > batch;
    fetched.truncate(batch);

    let covered = fetched
        .iter()
        .map(uid_of)
        .max()
        .map_or(known_high, |max| max.max(known_high));
    let highest_uid = (covered > 0).then_some(covered);

    // Removed: UIDs at or below the known high-water mark that the server
    // no longer reports. The engine intersects this with the UIDs it
    // actually stored, so reporting numbers that were never ingested (the
    // gaps between assigned UIDs) is harmless.
    let present = uid_search_all(connected, timeout).await?;
    let present_set: HashSet<u32> = present.iter().copied().collect();
    let removed = (1..=known_high)
        .filter(|uid| !present_set.contains(uid))
        .map(|uid| RemoteMessageId::Imap { uid, uid_validity })
        .collect();

    let flag_updates = fetch_flag_updates(connected, known_high, uid_validity, timeout).await?;

    Ok(SyncOutcome {
        uid_validity: Some(uid_validity),
        highest_uid,
        has_more,
        new_envelopes: fetched,
        flag_updates,
        removed,
    })
}

/// Fetches current flags for a bounded window of the highest known UIDs.
///
/// The server returns entries only for messages that still exist, so the
/// result already excludes removed messages. The window is capped at
/// [`FLAG_WINDOW`]; older messages are not re-checked for flag changes.
async fn fetch_flag_updates(
    connected: &mut Connected,
    known_high: u32,
    uid_validity: u32,
    timeout: Duration,
) -> Result<Vec<(RemoteMessageId, MessageFlags)>, MailError> {
    if known_high == 0 {
        return Ok(Vec::new());
    }
    let low = known_high.saturating_sub(FLAG_WINDOW - 1).max(1);
    let range = format!("{low}:{known_high}");
    let fetches = with_deadline(timeout, async {
        let stream = connected
            .session
            .uid_fetch(range, "(FLAGS)")
            .await
            .map_err(|err| map_imap_error(&err))?;
        stream
            .try_collect::<Vec<_>>()
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await?;

    Ok(fetches
        .iter()
        .filter_map(|fetch| {
            fetch.uid.map(|uid| {
                (
                    RemoteMessageId::Imap { uid, uid_validity },
                    flags_from_fetch(fetch),
                )
            })
        })
        .collect())
}

/// Runs `UID SEARCH ALL`, returning the matching UIDs in ascending order.
///
/// The server may return them in any order (async-imap collects them into a
/// set), so sorting is required for the "first `batch` from UID 1" slice.
async fn uid_search_all(
    connected: &mut Connected,
    timeout: Duration,
) -> Result<Vec<u32>, MailError> {
    let set = with_deadline(timeout, async {
        connected
            .session
            .uid_search("ALL")
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await?;
    let mut uids: Vec<u32> = set.into_iter().collect();
    uids.sort_unstable();
    Ok(uids)
}

/// Fetches envelopes for a UID set expression and converts them.
async fn fetch_envelopes(
    connected: &mut Connected,
    uid_set: &str,
    uid_validity: u32,
    timeout: Duration,
) -> Result<Vec<RemoteEnvelope>, MailError> {
    let fetches = with_deadline(timeout, async {
        let stream = connected
            .session
            .uid_fetch(uid_set, ENVELOPE_ITEMS)
            .await
            .map_err(|err| map_imap_error(&err))?;
        stream
            .try_collect::<Vec<_>>()
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await?;

    Ok(fetches
        .iter()
        .filter_map(|fetch| envelope_from_fetch(fetch, uid_validity))
        .collect())
}

/// Extracts the UID from a converted envelope (always an IMAP identity).
fn uid_of(envelope: &RemoteEnvelope) -> u32 {
    match envelope.remote_id {
        RemoteMessageId::Imap { uid, .. } => uid,
        RemoteMessageId::Pop3 { .. } => 0,
    }
}
