//! CONDSTORE / QRESYNC incremental synchronization (RFC 7162).
//!
//! This is the mod-sequence delta strategy [`crate::sync::run`] prefers when
//! the server advertises `CONDSTORE` (or `QRESYNC`, which subsumes it):
//!
//! - [`select`] issues `SELECT (CONDSTORE)`, recording the mailbox's
//!   `HIGHESTMODSEQ`, and enables `QRESYNC` once per connection.
//! - [`incremental`] fetches only the flags that changed since the stored
//!   mod-sequence via `UID FETCH … (CHANGEDSINCE <modseq>)`, replacing the
//!   legacy newest-UID flag window (and, with it, the per-pass keyword
//!   reconcile write-amplification, since untouched messages are never
//!   re-reported).
//!
//! Removals follow the strongest safe source:
//!
//! - **QRESYNC**: the `VANISHED (EARLIER)` response to the fetch names the
//!   expunged UIDs directly, so no `UID SEARCH ALL` present-set diff is needed.
//! - **CONDSTORE-only, a periodic reconcile pass, or any VANISHED ambiguity**:
//!   fall back to the authoritative present-set diff. Deletions are never
//!   invented — a message is only removed when a trusted source confirms it
//!   vanished, and when in doubt the present-set diff (bounded by stored rows)
//!   is the arbiter.

use std::time::Duration;

use async_imap::imap_proto::Response;
use async_imap::types::{Mailbox, UnsolicitedResponse};
use axomail_core::{
    ErrorClass, MailError, MailboxSyncState, MessageFlags, RemoteMessageId, SyncOutcome,
};
use futures::TryStreamExt;

use crate::connection::Connected;
use crate::convert::flags_from_fetch;
use crate::error::{map_imap_error, with_deadline};
use crate::sync::{fetch_new_messages, imap_ids, uid_search_all};

/// Guard against a pathological `VANISHED (EARLIER)` range: beyond this many
/// UIDs the response is treated as ambiguous and the present-set diff
/// (bounded by stored rows) arbitrates instead.
const MAX_VANISHED: usize = 50_000;

/// Whether the stored mod-sequence can drive a `CHANGEDSINCE` delta.
///
/// It must exist and the server's current `HIGHESTMODSEQ` must be present and
/// not have gone backwards; a `NOMODSEQ` select (server modseq `None`) or a
/// mod-sequence reset both force the legacy path, which re-records the server's
/// value so a later pass can resume the delta.
#[must_use]
pub(crate) fn modseq_usable(state: &MailboxSyncState, server_modseq: Option<u64>) -> bool {
    matches!(
        (state.highest_modseq, server_modseq),
        (Some(stored), Some(server)) if server >= stored
    )
}

/// Selects the mailbox, using `SELECT (CONDSTORE)` when the server supports a
/// mod-sequence so the returned [`Mailbox::highest_modseq`] is populated, and
/// enabling `QRESYNC` once per connection beforehand.
pub(crate) async fn select(
    connected: &mut Connected,
    encoded_name: &str,
    condstore: bool,
    qresync: bool,
    timeout: Duration,
) -> Result<Mailbox, MailError> {
    if qresync && !connected.qresync_attempted {
        maybe_enable_qresync(connected, timeout).await?;
    }
    with_deadline(timeout, async {
        let result = if condstore {
            connected.session.select_condstore(encoded_name).await
        } else {
            connected.session.select(encoded_name).await
        };
        result.map_err(|err| map_imap_error(&err))
    })
    .await
}

/// Issues `ENABLE QRESYNC` best-effort, at most once per connection.
///
/// A connection-level (temporary) failure propagates so the pass retries and
/// leaves the attempt un-recorded — a fresh connection may try again. A
/// definitive outcome (enabled, or a server rejection) marks the attempt so the
/// sync path never re-issues `ENABLE` on a later, already-selected pass (RFC
/// 3501 forbids `ENABLE` outside authenticated-not-selected). A rejection
/// simply degrades the connection to CONDSTORE-only.
async fn maybe_enable_qresync(
    connected: &mut Connected,
    timeout: Duration,
) -> Result<(), MailError> {
    let result = with_deadline(timeout, async {
        connected
            .session
            .run_command_and_check_ok("ENABLE QRESYNC")
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await;
    match result {
        Ok(()) => {
            connected.qresync_enabled = true;
            connected.qresync_attempted = true;
            Ok(())
        }
        // Temporary failure: do not record the attempt, so a reconnected pass
        // can retry `ENABLE` from the authenticated-not-selected state.
        Err(error) if error.class() == ErrorClass::Temporary => Err(error),
        Err(error) => {
            // Server rejection: record the attempt and stay CONDSTORE-only for
            // the rest of this connection rather than re-issuing ENABLE selected.
            tracing::warn!(error = %error, "ENABLE QRESYNC rejected; using CONDSTORE-only sync");
            connected.qresync_attempted = true;
            Ok(())
        }
    }
}

/// Computes the incremental delta with a mod-sequence.
///
/// `server_modseq` (the `HIGHESTMODSEQ` observed at select) is recorded so the
/// next pass advances its cursor. `qresync` selects VANISHED removals;
/// `full_reconcile` forces the present-set safety net for this pass.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn incremental(
    connected: &mut Connected,
    uid_validity: u32,
    server_modseq: Option<u64>,
    state: &MailboxSyncState,
    qresync: bool,
    full_reconcile: bool,
    batch: usize,
    timeout: Duration,
) -> Result<SyncOutcome, MailError> {
    let known_high = state.highest_uid.unwrap_or(0);
    // `modseq_usable` guaranteed a stored value before this path was chosen.
    let stored_modseq = state.highest_modseq.unwrap_or(0);

    let new = fetch_new_messages(connected, known_high, uid_validity, batch, timeout).await?;

    // VANISHED is only requested on a trusted fast pass. A periodic reconcile
    // pass omits it and lets the present-set diff arbitrate instead.
    let want_vanished = qresync && !full_reconcile;
    let (flag_updates, vanished) = changed_since(
        connected,
        known_high,
        stored_modseq,
        uid_validity,
        want_vanished,
        timeout,
    )
    .await?;

    let (removed, present_remote_ids) = if want_vanished {
        match vanished {
            // Trust the server's expunge delta: delete exactly these UIDs and
            // skip the present-set scan.
            Vanished::Earlier(uids) => (imap_ids(&uids, uid_validity), None),
            // No VANISHED means nothing was expunged since the cursor; the
            // periodic reconcile pass backstops any silently missed deletion.
            Vanished::None => (Vec::new(), None),
            // Anything the server sent that we cannot trust as an EARLIER delta
            // falls back to the authoritative diff — never a guessed deletion.
            Vanished::Ambiguous => (
                Vec::new(),
                Some(present_set(connected, uid_validity, timeout).await?),
            ),
        }
    } else {
        // CONDSTORE-only, or a QRESYNC periodic reconcile pass: the present-set
        // diff is the authoritative removal source.
        (
            Vec::new(),
            Some(present_set(connected, uid_validity, timeout).await?),
        )
    };

    Ok(SyncOutcome {
        uid_validity: Some(uid_validity),
        highest_uid: new.highest_uid,
        highest_modseq: server_modseq,
        has_more: new.has_more,
        new_envelopes: new.envelopes,
        flag_updates,
        removed,
        present_remote_ids,
    })
}

/// Runs `UID SEARCH ALL` and maps it to the authoritative present set.
async fn present_set(
    connected: &mut Connected,
    uid_validity: u32,
    timeout: Duration,
) -> Result<Vec<RemoteMessageId>, MailError> {
    let present = uid_search_all(connected, timeout).await?;
    Ok(imap_ids(&present, uid_validity))
}

/// The outcome of draining `VANISHED (EARLIER)` after a CHANGEDSINCE fetch.
enum Vanished {
    /// No VANISHED response (or none requested): nothing expunged.
    None,
    /// A clean `(EARLIER)` delta naming the expunged UIDs.
    Earlier(Vec<u32>),
    /// A response we cannot trust as an EARLIER delta; the caller must fall
    /// back to the present-set diff rather than delete anything.
    Ambiguous,
}

/// Fetches `(FLAGS)` for all known UIDs changed since `modseq`, optionally with
/// the `VANISHED` modifier, and reads any resulting `VANISHED (EARLIER)`
/// response from the unsolicited-responses channel.
async fn changed_since(
    connected: &mut Connected,
    known_high: u32,
    modseq: u64,
    uid_validity: u32,
    want_vanished: bool,
    timeout: Duration,
) -> Result<(Vec<(RemoteMessageId, MessageFlags)>, Vanished), MailError> {
    if known_high == 0 {
        // No previously known messages: no flag deltas and nothing to expunge.
        return Ok((Vec::new(), Vanished::None));
    }

    let query = if want_vanished {
        format!("(FLAGS) (CHANGEDSINCE {modseq} VANISHED)")
    } else {
        format!("(FLAGS) (CHANGEDSINCE {modseq})")
    };
    let range = format!("1:{known_high}");

    // Clear any stale unsolicited responses so only this fetch's VANISHED is
    // observed, then fully consume the fetch stream (which routes VANISHED into
    // the channel) before draining it.
    drain_unsolicited(connected);
    let fetches = with_deadline(timeout, async {
        let stream = connected
            .session
            .uid_fetch(range, query)
            .await
            .map_err(|err| map_imap_error(&err))?;
        stream
            .try_collect::<Vec<_>>()
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await?;

    let flag_updates = fetches
        .iter()
        .filter_map(|fetch| {
            fetch.uid.map(|uid| {
                (
                    RemoteMessageId::Imap { uid, uid_validity },
                    flags_from_fetch(fetch),
                )
            })
        })
        .collect();

    let vanished = if want_vanished {
        collect_vanished(connected, known_high)
    } else {
        Vanished::None
    };
    Ok((flag_updates, vanished))
}

/// Discards any buffered unsolicited responses.
fn drain_unsolicited(connected: &Connected) {
    while connected.session.unsolicited_responses.try_recv().is_ok() {}
}

/// Reads `VANISHED` responses from the unsolicited channel and classifies them
/// against the known UID range (see [`classify_vanished`]).
fn collect_vanished(connected: &Connected, known_high: u32) -> Vanished {
    let mut responses = Vec::new();
    while let Ok(response) = connected.session.unsolicited_responses.try_recv() {
        let UnsolicitedResponse::Other(data) = &response else {
            continue;
        };
        if let Response::Vanished { earlier, uids } = data.parsed() {
            responses.push((*earlier, uids.clone()));
        }
    }
    classify_vanished(&responses, known_high)
}

/// Classifies drained `(earlier, ranges)` VANISHED responses into a removal
/// decision, clamped to `1..=known_high`.
///
/// Anything that is not a trustworthy `(EARLIER)` resync delta — a real-time
/// VANISHED (no EARLIER), a range entirely outside the known UID space, or an
/// oversized range — yields [`Vanished::Ambiguous`] so the caller falls back to
/// the present-set diff instead of inventing a deletion.
fn classify_vanished(
    responses: &[(bool, Vec<std::ops::RangeInclusive<u32>>)],
    known_high: u32,
) -> Vanished {
    let mut uids = Vec::new();
    let mut saw_earlier = false;
    for (earlier, ranges) in responses {
        if !*earlier {
            return Vanished::Ambiguous;
        }
        saw_earlier = true;
        for range in ranges {
            let start = (*range.start()).max(1);
            let end = (*range.end()).min(known_high);
            if start > end {
                // A range entirely outside the known UID space is anomalous.
                return Vanished::Ambiguous;
            }
            let span = (end - start) as usize + 1;
            if uids.len().saturating_add(span) > MAX_VANISHED {
                return Vanished::Ambiguous;
            }
            uids.extend(start..=end);
        }
    }
    if saw_earlier {
        Vanished::Earlier(uids)
    } else {
        Vanished::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(modseq: Option<u64>) -> MailboxSyncState {
        MailboxSyncState {
            uid_validity: Some(42),
            highest_uid: Some(10),
            highest_modseq: modseq,
        }
    }

    #[test]
    fn modseq_usable_requires_present_non_regressing_values() {
        // Both present and the server not behind the cursor: usable.
        assert!(modseq_usable(&state(Some(10)), Some(20)));
        assert!(modseq_usable(&state(Some(10)), Some(10)));
        // No stored cursor yet (first CONDSTORE pass): not usable.
        assert!(!modseq_usable(&state(None), Some(20)));
        // Server reports NOMODSEQ: not usable.
        assert!(!modseq_usable(&state(Some(10)), None));
        // Mod-sequence went backwards (reset): not usable.
        assert!(!modseq_usable(&state(Some(10)), Some(5)));
    }

    #[test]
    fn classify_vanished_reads_clean_earlier_delta() {
        let responses = vec![(true, vec![3..=3, 7..=9])];
        match classify_vanished(&responses, 10) {
            Vanished::Earlier(uids) => assert_eq!(uids, vec![3, 7, 8, 9]),
            other => panic!("expected Earlier, got {}", label(&other)),
        }
    }

    #[test]
    fn classify_vanished_clamps_partial_overlap_to_known_range() {
        // 8:15 with known_high 10 keeps 8,9,10 and drops the unknown tail.
        let responses = vec![(true, vec![8..=15])];
        match classify_vanished(&responses, 10) {
            Vanished::Earlier(uids) => assert_eq!(uids, vec![8, 9, 10]),
            other => panic!("expected Earlier, got {}", label(&other)),
        }
    }

    #[test]
    fn classify_vanished_treats_non_earlier_as_ambiguous() {
        let responses = vec![(false, vec![3..=3])];
        assert!(matches!(
            classify_vanished(&responses, 10),
            Vanished::Ambiguous
        ));
    }

    #[test]
    fn classify_vanished_treats_out_of_range_as_ambiguous() {
        let responses = vec![(true, vec![100..=200])];
        assert!(matches!(
            classify_vanished(&responses, 10),
            Vanished::Ambiguous
        ));
    }

    #[test]
    fn classify_vanished_treats_oversized_range_as_ambiguous() {
        let responses = vec![(true, vec![1..=u32::MAX])];
        assert!(matches!(
            classify_vanished(&responses, u32::MAX),
            Vanished::Ambiguous
        ));
    }

    #[test]
    fn classify_vanished_reports_none_when_empty() {
        assert!(matches!(classify_vanished(&[], 10), Vanished::None));
    }

    fn label(vanished: &Vanished) -> &'static str {
        match vanished {
            Vanished::None => "None",
            Vanished::Earlier(_) => "Earlier",
            Vanished::Ambiguous => "Ambiguous",
        }
    }
}
