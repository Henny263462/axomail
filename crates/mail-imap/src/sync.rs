//! One incremental synchronization pass over a mailbox.
//!
//! Selecting the mailbox yields `UIDVALIDITY`. If it differs from the
//! caller's last-known value (or there was none), the full current picture
//! is returned and the engine resets its cache. Otherwise only the delta
//! is computed: new messages, flag changes, and removals.
//!
//! Two incremental strategies exist, chosen from the server's capabilities:
//!
//! - **CONDSTORE/QRESYNC** ([`crate::condstore`]) — a mod-sequence delta:
//!   `SELECT (CONDSTORE)` records `HIGHESTMODSEQ`, and a `CHANGEDSINCE` fetch
//!   returns only the flags that changed. Under QRESYNC a `VANISHED (EARLIER)`
//!   response supplies removals directly; otherwise removals come from the
//!   authoritative present-set diff.
//! - **Legacy** (this module, [`incremental`]) — the capability-free path used
//!   when the server advertises neither: a bounded newest-UID flag window plus
//!   the present-set diff. Behaviour here is unchanged.

use std::time::Duration;

use axomail_core::{
    MailError, MailboxSyncState, MessageFlags, RemoteEnvelope, RemoteMessageId, SyncOutcome,
    SyncRequest,
};
use futures::TryStreamExt;

use crate::connection::Connected;
use crate::convert::{envelope_from_fetch, flags_from_fetch, format_uid_set};
use crate::error::{map_imap_error, with_deadline};
use crate::{condstore, utf7};

/// FETCH items requested for every envelope.
const ENVELOPE_ITEMS: &str = "(ENVELOPE FLAGS RFC822.SIZE INTERNALDATE)";

/// Upper bound on how many of the highest known UIDs are re-checked for
/// flag changes each pass on the **legacy** (no-CONDSTORE) path. Without a
/// mod-sequence the whole known range would otherwise be re-fetched; this
/// caps the cost while still catching recent changes. The CONDSTORE path
/// ([`crate::condstore`]) replaces this heuristic with a `CHANGEDSINCE` delta.
const FLAG_WINDOW: u32 = 500;

/// Runs a synchronization pass and returns the outcome to persist.
pub(crate) async fn run(
    connected: &mut Connected,
    request: &SyncRequest,
    timeout: Duration,
) -> Result<SyncOutcome, MailError> {
    tracing::debug!(mailbox = %request.remote_name, "starting IMAP sync pass");
    let encoded = utf7::encode(request.remote_name.as_str());

    // QRESYNC implies CONDSTORE (RFC 7162 §3.2.3), so either capability enables
    // the mod-sequence select; QRESYNC additionally unlocks VANISHED removals.
    let condstore = connected.has_capability("CONDSTORE") || connected.has_capability("QRESYNC");
    let qresync = connected.has_capability("QRESYNC");

    let mailbox = condstore::select(connected, &encoded, condstore, qresync, timeout).await?;
    let uid_validity = mailbox.uid_validity.ok_or_else(|| MailError::Protocol {
        message: "the server did not report UIDVALIDITY on SELECT".to_owned(),
    })?;
    let server_modseq = mailbox.highest_modseq;
    // `ENABLE QRESYNC` may have been declined by the server; only a genuinely
    // enabled connection uses VANISHED removals.
    let qresync_active = connected.qresync_enabled;

    let batch = request.batch_size.max(1) as usize;
    if request.state.uid_validity != Some(uid_validity) {
        return full_picture(connected, uid_validity, server_modseq, batch, timeout).await;
    }

    // A usable stored mod-sequence (present, and not gone backwards versus the
    // server) unlocks the CONDSTORE delta; otherwise fall back to the legacy
    // window scan while still recording the server's mod-sequence so the next
    // pass can upgrade.
    if condstore && condstore::modseq_usable(&request.state, server_modseq) {
        condstore::incremental(
            connected,
            uid_validity,
            server_modseq,
            &request.state,
            qresync_active,
            request.full_reconcile,
            batch,
            timeout,
        )
        .await
    } else {
        incremental(
            connected,
            uid_validity,
            server_modseq,
            &request.state,
            batch,
            timeout,
        )
        .await
    }
}

/// Returns the first `batch` messages as a complete snapshot (used on the
/// first sync and whenever `UIDVALIDITY` changed).
async fn full_picture(
    connected: &mut Connected,
    uid_validity: u32,
    server_modseq: Option<u64>,
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
        // Record the mod-sequence (when the server supports it) so the next
        // pass can request a CHANGEDSINCE delta instead of a full scan.
        highest_modseq: server_modseq,
        has_more,
        new_envelopes,
        flag_updates: Vec::new(),
        removed: Vec::new(),
        // Full-picture path: the caller wipes local state on a validity
        // change, so no present-set diff is needed here.
        present_remote_ids: None,
    })
}

/// Computes the delta since the last pass under an unchanged `UIDVALIDITY`,
/// using the legacy capability-free strategy (bounded flag window + present-set
/// diff). `server_modseq` is recorded so a CONDSTORE-capable server bootstraps
/// its mod-sequence cursor on the way through.
async fn incremental(
    connected: &mut Connected,
    uid_validity: u32,
    server_modseq: Option<u64>,
    state: &MailboxSyncState,
    batch: usize,
    timeout: Duration,
) -> Result<SyncOutcome, MailError> {
    let known_high = state.highest_uid.unwrap_or(0);
    let new = fetch_new_messages(connected, known_high, uid_validity, batch, timeout).await?;

    // Removals: report the FULL authoritative present set (bounded by the real
    // message count) instead of enumerating absent UIDs — a sparse UID space
    // would otherwise explode into millions of phantom entries. The set must
    // NOT be capped at `known_high`: the caller diffs every stored row against
    // it in the same transaction that inserts this pass's new envelopes, so
    // omitting the new UIDs would delete them right after insertion.
    let present = uid_search_all(connected, timeout).await?;
    let present_remote_ids = Some(imap_ids(&present, uid_validity));

    let flag_updates = fetch_flag_updates(connected, known_high, uid_validity, timeout).await?;

    Ok(SyncOutcome {
        uid_validity: Some(uid_validity),
        highest_uid: new.highest_uid,
        highest_modseq: server_modseq,
        has_more: new.has_more,
        new_envelopes: new.envelopes,
        flag_updates,
        removed: Vec::new(),
        present_remote_ids,
    })
}

/// New messages discovered above the known high-water mark in one pass.
pub(crate) struct NewMessages {
    /// Newly discovered envelopes, sorted ascending and truncated to `batch`.
    pub envelopes: Vec<RemoteEnvelope>,
    /// Whether more new messages remain beyond this batch.
    pub has_more: bool,
    /// Highest UID covered after this pass (never below `known_high`).
    pub highest_uid: Option<u32>,
}

/// Fetches envelopes strictly above `known_high`, shared by the legacy and
/// CONDSTORE incremental strategies (new-message discovery is identical; only
/// the flag/removal delta differs between them).
pub(crate) async fn fetch_new_messages(
    connected: &mut Connected,
    known_high: u32,
    uid_validity: u32,
    batch: usize,
    timeout: Duration,
) -> Result<NewMessages, MailError> {
    // The `N:*` form can re-return the single highest message when N exceeds
    // it, so results are filtered to `uid > known_high`.
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

    Ok(NewMessages {
        envelopes: fetched,
        has_more,
        highest_uid,
    })
}

/// Fetches current flags for a bounded window of the highest known UIDs
/// (legacy, no-CONDSTORE path).
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
pub(crate) async fn uid_search_all(
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
pub(crate) async fn fetch_envelopes(
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

/// Maps a slice of UIDs to IMAP remote identities under one `UIDVALIDITY`.
/// Used both for the authoritative present set and for VANISHED removals.
#[must_use]
pub(crate) fn imap_ids(uids: &[u32], uid_validity: u32) -> Vec<RemoteMessageId> {
    uids.iter()
        .copied()
        .map(|uid| RemoteMessageId::Imap { uid, uid_validity })
        .collect()
}

/// Extracts the UID from a converted envelope (always an IMAP identity).
pub(crate) fn uid_of(envelope: &RemoteEnvelope) -> u32 {
    match envelope.remote_id {
        RemoteMessageId::Imap { uid, .. } => uid,
        RemoteMessageId::Pop3 { .. } => 0,
    }
}
