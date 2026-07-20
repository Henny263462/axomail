# 0014. IMAP IDLE push on a dedicated, read-only connection

Date: 2026-07-20
Status: accepted

## Context

Wave 4 track 4.7 (cluster E2) adds IMAP IDLE (RFC 2177) so new mail triggers an
**immediate** sync instead of waiting for the next poll (current-state matrix,
Sync engine: "IMAP IDLE / push — Missing, poll-only, 60s floor"). Three
constraints shape the design:

- **One-connection-per-account sync model.** The engine serializes an account's
  jobs behind a per-account lock and each job opens its own short-lived
  connection. IDLE is long-lived (it blocks for up to ~29 minutes), so routing
  it through a normal job would hold the account lock indefinitely and starve
  sync and write-back. The Wave-1 review flagged `detail.rs` opening a second
  connection outside the lock as a latent hazard; IDLE must not repeat an unsafe
  variant of that.
- **`async-imap` IDLE consumes the session.** `Session::idle()` takes the
  session by value and returns it only on `DONE`. IDLE therefore cannot share
  the borrow-per-op shape the other adapter methods use.
- **Protocol-agnostic engine (ADR-0004).** `mail-sync` must not name
  `axomail-imap`; POP3 has no push.

## Decision

**A dedicated, read-only IDLE connection per account, outside the job lock.**
A new `PushSupervisor` in `mail-sync` runs alongside the `SyncEngine` and the
`BackgroundScheduler`. For each target it spawns a task that opens its **own**
connection — separate from the engine's sync/write-back connection and held
outside the per-account job lock. That connection only ever `SELECT`s the inbox
and `IDLE`s: it never mutates the mailbox, so it cannot contend with the
write-back connection for one session. When the server pushes a change the
supervisor enqueues a normal `SyncMailbox` job, which runs on the engine's
connection under the usual lock. Two concurrent connections per account (idle +
sync) is well within typical server limits and is how Thunderbird behaves.

**Push is an accelerator, not a replacement.** The poller keeps running as a
backstop, so an account whose server lacks IDLE, or whose push is disabled or
offline, simply keeps polling. There is no separate "poll vs push" mode to get
wrong: IDLE only ever *adds* an immediate-sync trigger.

**Capability-gated via the provider trait.** `IncomingMailProvider` gains two
methods with **default** implementations (the first defaulted methods on the
trait, deliberately, so the many non-IMAP implementors and test doubles need no
boilerplate): `supports_idle() -> bool` (default `false`) and
`idle(max_wait, interrupt) -> IdleOutcome` (default `UnsupportedCapability`).
IMAP overrides both; the dispatch enum overrides them to reach the IMAP arm;
POP3 and the fakes inherit the defaults. `idle` takes the cancellation as an
`impl Future<Output = ()>` argument rather than a `tokio_util` token, so
`mail-core` gains no async-runtime dependency.

**A clean `DONE` on every non-error return.** `idle` waits on the currently
selected mailbox until the server pushes a change (`Notified` → sync now), the
`max_wait` renewal deadline elapses (`TimedOut` → re-issue IDLE before the
server's ~29-minute limit), or the caller's interrupt fires (`Interrupted` →
shutdown/cancel). In all three it sends `DONE`, leaving the session reusable. A
transport error drops the session and propagates; the supervisor reconnects with
the engine's equal-jitter backoff, resetting the backoff after each successful
connection.

**Status on the existing event channel.** Connection state rides the engine's
`SyncEvent` broadcast as a `PushStateChanged { connected | polling |
reconnecting }` variant, forwarded to the frontend on a dedicated
`axomail://push` Tauri channel (the job-centric `axomail://sync` DTO is keyed by
job id, which push has none of). A `push_enabled` setting (default on) gates the
whole supervisor; the Work Offline flag disables it too (offline means no
network connections at all).

## Consequences

- New mail on an IDLE-capable server appears within seconds, without shortening
  the poll interval.
- The IDLE connection never issues a mutating command, so the "second connection
  outside the lock" is safe by construction — it cannot corrupt sync/write-back
  state the way an out-of-band mutation could.
- POP3, IDLE-less IMAP servers, disabled push, and offline mode all degrade to
  the pre-existing polling behaviour with no code path change.
- The first use of trait-default methods on `IncomingMailProvider`: a capability
  that most implementors legitimately do not have expresses its absence once, in
  the default, instead of in every adapter and test double.
