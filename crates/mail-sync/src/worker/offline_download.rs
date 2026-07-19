//! Sync-time offline body download (track 2.3).
//!
//! After a mailbox's envelope sync completes, this bounded pass downloads the
//! full raw bodies of the messages the mailbox's offline policy covers, caches
//! them in the raw store, flips `has_raw`, and indexes the full body text for
//! search — so an offline reader (and search) has the whole message without
//! ever opening it. It reuses the still-connected sync provider, so it stays
//! under the same per-account lock and single connection as the sync pass, and
//! it is capped per job ([`SyncConfig::offline_download_batch`]) so a large
//! backlog drains incrementally over successive passes.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axomail_core::{IncomingMailProvider, MailError, MailboxId};
use axomail_parser::parse_message;
use axomail_storage::raw_message_path;
use tokio_util::sync::CancellationToken;

use crate::factory::ProviderFactory;
use crate::shared::Shared;

use super::guarded;

/// Downloads and caches up to one batch of policy-covered bodies for `mailbox`,
/// using the already-connected `provider`. Returns how many bodies were cached.
///
/// A `None` raw root (offline store disabled) or a policy that covers nothing
/// is a no-op returning `0`. Cancellation propagates as [`MailError::Cancelled`];
/// a per-message fetch failure ends the pass early (the bodies already cached
/// stay committed and the rest retry on the next pass) without failing the job.
pub(super) async fn run<P, F>(
    shared: &Arc<Shared<P, F>>,
    mailbox: MailboxId,
    provider: &mut P,
    token: &CancellationToken,
) -> Result<u64, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let Some(raw_root) = shared.raw_root.clone() else {
        return Ok(0);
    };

    let limit = shared.config.offline_download_batch;
    if limit == 0 {
        return Ok(0);
    }
    let plan = shared
        .store
        .offline()
        .download_plan(mailbox, now_unix(), limit)
        .await?;

    let requested = plan.candidates.len();
    let net = shared.config.network_timeout;
    let mut cached = 0u64;
    let mut completed_all = true;
    for candidate in plan.candidates {
        if token.is_cancelled() {
            return Err(MailError::Cancelled);
        }
        let raw = match guarded(token, net, provider.fetch_message(&candidate.remote_id)).await {
            Ok(raw) => raw,
            Err(MailError::Cancelled) => return Err(MailError::Cancelled),
            Err(error) => {
                // A body fetch failing is not a sync failure: stop this pass
                // and let the remaining bodies retry on the next one.
                tracing::warn!(error = %error, "offline body download failed; deferring rest");
                completed_all = false;
                break;
            }
        };

        let path = raw_message_path(&raw_root, candidate.message_id.value());
        axomail_storage::write_raw_atomically(&path, &raw.bytes).await?;
        let body = parse_message(&raw.bytes)
            .ok()
            .and_then(|parsed| parsed.text_body)
            .unwrap_or_default();
        shared
            .store
            .offline()
            .mark_downloaded(candidate.message_id, &body)
            .await?;
        cached += 1;
    }

    // A "download now" override is cleared once its backlog is drained: the
    // batch came back short of the limit and every candidate was processed.
    if plan.force_full && completed_all && requested < limit as usize {
        shared
            .store
            .offline()
            .set_pending_full_download(mailbox, false)
            .await?;
    }

    Ok(cached)
}

/// Current wall-clock time in Unix seconds (0 before the epoch).
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
