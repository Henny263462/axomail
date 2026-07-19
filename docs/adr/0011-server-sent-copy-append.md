# 0011. Server-side Sent copy via IMAP APPEND with UID adoption

Date: 2026-07-19
Status: accepted

## Context

Wave 3 track 3.3 (cluster C2) files a copy of every sent message into the
account's Sent folder **on the server** so sent mail is visible from every
device, closing the "local Sent copy only" gap (current-state matrix, Compose /
send). Two constraints shape the design:

- **No `APPENDUID`.** `async-imap` 0.11's `append` returns `()`; it discards the
  `APPENDUID` response, so the UID the server assigned the appended message is
  not directly available.
- **No migration this wave.** Migration `0014` is reserved and expected unused,
  so no new schema (e.g. a durable "append" op-queue kind, or a server-UID
  column on drafts) may be added.

Envelope sync carries no `Message-ID` (IMAP threading is backfilled lazily on
first open), so a later sync of the Sent folder cannot recognize an
already-filed message by `Message-ID` at ingest time. Without a matching key the
downloaded server copy would be inserted as a **second** row alongside the
local-only copy — a double-file.

## Decision

**Render once, share the bytes.** The compose pipeline renders the outgoing
message a single time (`axomail_smtp::render_message` → bytes + `Message-ID`) and
hands the identical bytes to SMTP submission (`SmtpProvider::send_body`) and to
the Sent-copy `APPEND`. The delivered message and the filed copy are therefore
byte-for-byte identical and share one `Message-ID`.

**A capability-agnostic `append_message` trait method.** `IncomingMailProvider`
gains `append_message(mailbox, flags, raw) -> AppendOutcome`. IMAP issues plain
RFC 3501 `APPEND` with the flag list (`(\Seen)` for Sent, `(\Draft)` for a
draft); POP3 returns `UnsupportedCapability`; every adapter, the dispatch enum,
the POP3-sync passthrough, and the fakes implement it (ADR-0004 recipe).

**UID adoption prevents the double-file.** After a successful `APPEND` the IMAP
adapter resolves the appended message's UID **best-effort** by selecting the
target and issuing `UID SEARCH HEADER MESSAGE-ID "<id>"` (matching by
`Message-ID`, since `APPENDUID` is unavailable), and returns it in
`AppendOutcome.remote_id`. The send path then calls
`MessageRepo::adopt_remote_id`, which writes that identity onto the local Sent
row (guarded to rows still lacking one). The ingest unique index over
`(mailbox_id, remote_uid_validity, remote_uid)` then skips re-inserting the
message on the next sync. Failing UID resolution never fails the append — the
message is already stored — it only means the local copy is not adopted (a
future sync could then double-file, the safe-but-imperfect direction).

**Best-effort, off the UI path.** The Sent-copy upload is a post-send step: SMTP
already delivered the mail, so an `APPEND` failure is logged and swallowed, never
failing the send, and the local Sent copy always remains. The interactive send
commands (`send_message`, `retry_outbox`, `send_outbox_now`) spawn the upload on
a detached task so a slow or unreachable IMAP server does not block the composer.
The Sent folder is resolved from the sending identity's `sent_dest` (Wave 1),
falling back to the `\Sent`-role mailbox (Wave 2). POP3 accounts and accounts
with no resolvable Sent folder file locally only, with no error.

## Consequences

- Sent mail sent from an interactive command lands on the server and is
  recognized (not duplicated) by a later Sent-folder sync.
- **The automatic background outbox drain files locally only.** Offline-queued
  and scheduled ("send later") messages that the startup resume / send-later
  poller submit still file a local Sent copy but do **not** push to the server,
  because those paths run without a provider factory and a durable
  server-`APPEND` op-queue would need a new `pending_ops` kind (migration,
  deferred). The interactive "Send now" / retry buttons cover the server upload.
- **Drafts-to-server is deferred.** `append_message` fully supports the `\Draft`
  flag (adapter-tested), but wiring `save_draft` to mirror drafts into the server
  Drafts folder is not shipped this wave: clean *replacement* of a prior server
  draft needs either a schema column (server UID / stable `Message-ID` on the
  drafts table — migration, forbidden) or a derived-`Message-ID` search-and-
  expunge scheme over possibly-incomplete drafts. Local drafts remain the source
  of truth; the primitive is ready for a follow-up.
- A `MOVE`-capable, `UIDPLUS`-style server is not required: `APPEND` and
  `UID SEARCH` are base IMAP. A server that rejects the `Sent`/`Drafts` folder,
  or whose search does not resolve the `Message-ID`, degrades gracefully to a
  local-only copy.
