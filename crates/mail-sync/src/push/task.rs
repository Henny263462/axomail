//! The per-account push task: connect, IDLE, react, and reconnect with backoff.
//!
//! One of these runs per enabled [`PushTarget`](super::PushTarget). It opens a
//! **dedicated** IDLE connection (separate from the sync/write-back connection,
//! outside the engine's per-account job lock), selects the target mailbox, and
//! loops on IDLE: a server push triggers an immediate [`MailboxSync`]; a renewal
//! timeout re-issues IDLE; an error drops through to a backed-off reconnect. The
//! connection only ever idles and reports — it never mutates the mailbox — so it
//! cannot contend with the write-back connection.

use std::sync::Arc;

use axomail_core::{IdleOutcome, IncomingMailProvider};
use tokio_util::sync::CancellationToken;

use crate::backoff::backoff_from;
use crate::engine::SyncEngine;
use crate::event::PushState;
use crate::factory::ProviderFactory;
use crate::job::SyncMailbox;

use super::{PushConfig, PushTarget};

/// Runs the push loop for one account until its token is cancelled.
///
/// Reconnects after any dropped connection with equal-jitter backoff (the same
/// formula the sync engine uses), resetting the backoff after each successful
/// connection. Exits early — without retrying — when the server does not
/// advertise IDLE (the poller covers that account instead).
pub(super) async fn run<P, F>(
    engine: Arc<SyncEngine<P, F>>,
    factory: Arc<F>,
    store: axomail_storage::Store,
    target: PushTarget,
    config: PushConfig,
    token: CancellationToken,
) where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mut attempt = 0u32;
    loop {
        if token.is_cancelled() {
            break;
        }
        match watch_once(&engine, &factory, &store, &target, &config, &token).await {
            WatchExit::Cancelled => break,
            WatchExit::Unsupported => {
                // No server push for this account: announce polling and stop.
                // The background scheduler keeps it fresh.
                engine.report_push_state(target.account_id, None, PushState::Polling);
                break;
            }
            WatchExit::ConnectFailed => {
                engine.report_push_state(target.account_id, None, PushState::Reconnecting);
                attempt = attempt.saturating_add(1);
                if sleep_or_cancel(attempt, &config, &token).await {
                    break;
                }
            }
            WatchExit::Disconnected => {
                // A previously live connection dropped: reconnect promptly (from
                // the first backoff step), growing the delay only if the next
                // connect also fails.
                engine.report_push_state(target.account_id, None, PushState::Reconnecting);
                attempt = 1;
                if sleep_or_cancel(attempt, &config, &token).await {
                    break;
                }
            }
        }
    }
}

/// How one connect+watch attempt ended.
enum WatchExit {
    /// The token was cancelled; stop the loop.
    Cancelled,
    /// The server does not advertise IDLE; fall back to polling and stop.
    Unsupported,
    /// Could not connect or select; back off and retry.
    ConnectFailed,
    /// Was connected and idling, then the connection dropped; reconnect.
    Disconnected,
}

/// Opens one dedicated IDLE connection and idles until it drops or is cancelled.
async fn watch_once<P, F>(
    engine: &Arc<SyncEngine<P, F>>,
    factory: &Arc<F>,
    store: &axomail_storage::Store,
    target: &PushTarget,
    config: &PushConfig,
    token: &CancellationToken,
) -> WatchExit
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let account = match store.accounts().get(target.account_id).await {
        Ok(Some(account)) => account,
        Ok(None) => return WatchExit::ConnectFailed,
        Err(error) => {
            tracing::warn!(error = %error, "push: could not load account");
            return WatchExit::ConnectFailed;
        }
    };

    let mut provider = match guard(token, factory.make(&account)).await {
        Guarded::Done(Ok(provider)) => provider,
        Guarded::Done(Err(error)) => {
            tracing::debug!(error = %error, "push: could not build provider");
            return WatchExit::ConnectFailed;
        }
        Guarded::Cancelled => return WatchExit::Cancelled,
    };

    match guard(token, provider.connect()).await {
        Guarded::Done(Ok(())) => {}
        Guarded::Done(Err(error)) => {
            tracing::debug!(error = %error, "push: connect failed");
            return WatchExit::ConnectFailed;
        }
        Guarded::Cancelled => {
            let _ = provider.disconnect().await;
            return WatchExit::Cancelled;
        }
    }

    if !provider.supports_idle() {
        let _ = provider.disconnect().await;
        return WatchExit::Unsupported;
    }

    match guard(token, provider.select_mailbox(&target.remote_name)).await {
        Guarded::Done(Ok(())) => {}
        Guarded::Done(Err(error)) => {
            tracing::debug!(error = %error, "push: select failed");
            let _ = provider.disconnect().await;
            return WatchExit::ConnectFailed;
        }
        Guarded::Cancelled => {
            let _ = provider.disconnect().await;
            return WatchExit::Cancelled;
        }
    }

    // Live: announce the push connection, then idle until it drops or cancels.
    engine.report_push_state(
        target.account_id,
        Some(target.mailbox_id),
        PushState::Connected,
    );
    idle_loop(engine, &mut provider, target, config, token).await
}

/// The steady-state IDLE loop on an already-selected mailbox.
async fn idle_loop<P, F>(
    engine: &Arc<SyncEngine<P, F>>,
    provider: &mut P,
    target: &PushTarget,
    config: &PushConfig,
    token: &CancellationToken,
) -> WatchExit
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    loop {
        if token.is_cancelled() {
            let _ = provider.disconnect().await;
            return WatchExit::Cancelled;
        }
        match provider.idle(config.renew_after, token.cancelled()).await {
            Ok(IdleOutcome::Notified) => {
                // New activity on the mailbox: enqueue an immediate sync. The
                // engine de-duplicates against any in-flight sync, so a burst of
                // pushes cannot pile up work.
                let job = SyncMailbox {
                    account_id: target.account_id,
                    mailbox_id: target.mailbox_id,
                };
                if let Err(error) = engine.enqueue(job).await {
                    tracing::debug!(error = %error, "push: sync enqueue rejected");
                }
            }
            // Renewal: the loop simply re-issues IDLE.
            Ok(IdleOutcome::TimedOut) => {}
            Ok(IdleOutcome::Interrupted) => {
                // Cancelled during the wait; the adapter already sent DONE.
                let _ = provider.disconnect().await;
                return WatchExit::Cancelled;
            }
            Err(error) => {
                tracing::debug!(error = %error, "push: idle connection dropped");
                return WatchExit::Disconnected;
            }
        }
    }
}

/// Sleeps for the `attempt`-th reconnect backoff, or returns `true` if the
/// token was cancelled while waiting (so the caller should stop).
async fn sleep_or_cancel(attempt: u32, config: &PushConfig, token: &CancellationToken) -> bool {
    let delay = backoff_from(attempt, config.reconnect_base, config.reconnect_cap);
    tokio::select! {
        biased;
        () = token.cancelled() => true,
        () = tokio::time::sleep(delay) => false,
    }
}

/// Outcome of a token-guarded future.
enum Guarded<T> {
    /// The future completed with this value.
    Done(T),
    /// The token fired first.
    Cancelled,
}

/// Awaits `future`, returning early if `token` is cancelled first.
async fn guard<T, Fut>(token: &CancellationToken, future: Fut) -> Guarded<T>
where
    Fut: Future<Output = T>,
{
    tokio::select! {
        biased;
        () = token.cancelled() => Guarded::Cancelled,
        value = future => Guarded::Done(value),
    }
}
