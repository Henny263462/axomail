//! IMAP IDLE (RFC 2177): a single interruptible waiting period on an already
//! selected mailbox.
//!
//! [`run`] consumes a [`Connected`] session, issues `IDLE`, and blocks until one
//! of three things happens: the server pushes an unsolicited change (returning
//! [`IdleOutcome::Notified`]), the `max_wait` renewal deadline elapses
//! ([`IdleOutcome::TimedOut`]), or the caller's `interrupt` future fires
//! ([`IdleOutcome::Interrupted`]). In every non-error case it sends `DONE`
//! before returning, handing the session back so the caller can idle again
//! (renewal) or select another mailbox. A protocol or transport error drops the
//! session and propagates, so the caller reconnects.
//!
//! This path never mutates the mailbox — it only waits and reports — so a
//! dedicated push connection can run outside the sync/write-back connection
//! lock (the "second connection per account" the engine expects for push).

use std::time::Duration;

use async_imap::extensions::idle::IdleResponse;
use axomail_core::{IdleOutcome, MailError};
use futures::future::{Either, select};

use crate::connection::Connected;
use crate::error::{map_imap_error, with_deadline};

/// Runs one IDLE waiting period on `connected`'s currently selected mailbox.
///
/// On success returns the recovered session (after a clean `DONE`) together
/// with how the wait ended. `interrupt` cancels the wait cooperatively; when it
/// resolves first the adapter still sends `DONE`, so the session stays usable
/// even on a cancel. `init_timeout` bounds the `IDLE`/`DONE` round-trips (not
/// the wait itself, which `max_wait` and `interrupt` bound).
pub(crate) async fn run<I>(
    connected: Connected,
    max_wait: Duration,
    interrupt: I,
    init_timeout: Duration,
) -> Result<(Connected, IdleOutcome), MailError>
where
    I: Future<Output = ()> + Send,
{
    let Connected {
        session,
        capabilities,
        qresync_enabled,
        qresync_attempted,
    } = connected;

    let mut handle = session.idle();
    // Send `IDLE` and await the server's continuation. A failure here drops the
    // handle (and its session): the connection is gone and the caller reconnects.
    with_deadline(init_timeout, async {
        handle.init().await.map_err(|err| map_imap_error(&err))
    })
    .await?;

    // Race the interruptible wait against the caller's interrupt future. Both
    // borrow `handle`; scoping them in this block releases those borrows before
    // the `DONE` call below, which consumes the handle by value.
    let waited: Result<IdleOutcome, MailError> = {
        let (wait, _stop) = handle.wait_with_timeout(max_wait);
        futures::pin_mut!(wait);
        futures::pin_mut!(interrupt);
        match select(wait, interrupt).await {
            Either::Left((response, _interrupt)) => match response {
                Ok(IdleResponse::Timeout) => Ok(IdleOutcome::TimedOut),
                Ok(IdleResponse::NewData(_)) => Ok(IdleOutcome::Notified),
                // The manual interrupt (stop token) is unused here — we cancel
                // via the `interrupt` future — so this only fires if the server
                // ended the stream, which we treat as a change to re-sync.
                Ok(IdleResponse::ManualInterrupt) => Ok(IdleOutcome::Notified),
                Err(err) => Err(map_imap_error(&err)),
            },
            Either::Right(((), _wait)) => Ok(IdleOutcome::Interrupted),
        }
    };
    // A wait error means the connection is broken: drop the handle (session)
    // and propagate so the caller reconnects. On success, `DONE` terminates the
    // IDLE cleanly and hands the session back for reuse.
    let outcome = waited?;
    let session = with_deadline(init_timeout, async {
        handle.done().await.map_err(|err| map_imap_error(&err))
    })
    .await?;

    Ok((
        Connected {
            session,
            capabilities,
            qresync_enabled,
            qresync_attempted,
        },
        outcome,
    ))
}
