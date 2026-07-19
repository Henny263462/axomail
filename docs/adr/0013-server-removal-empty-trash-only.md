# 0013. Server-side message removal happens only in Empty Trash

Date: 2026-07-20
Status: accepted

## Context

Wave 3 track 3.2 (cluster M2) adds the message-relocation actions (move, copy,
archive, delete-to-Trash) and the Trash lifecycle (Empty Trash, Compact). Two
of these can destroy mail irreversibly on the server, so the boundary between
*reversible* and *irreversible* actions must be explicit and hard to cross by
accident.

The assistant safety rules forbid an agent from "emptying trash" on a user's
behalf. This ADR concerns the **user's own explicit in-app action** on their own
mailbox, which is a legitimate product feature — but it is exactly the operation
the rules treat as dangerous, so it must be gated unmistakably and confined so a
bug cannot widen its blast radius.

Two IMAP hazards shape the design:

- **`EXPUNGE` is mailbox-scoped but blunt.** A plain `EXPUNGE` removes *every*
  `\Deleted` message in the **selected** mailbox. A message flagged `\Deleted`
  by the move COPY-fallback (on a server without `MOVE`) is a tombstone that a
  stray `EXPUNGE` would purge as collateral.
- **`UID EXPUNGE` needs UIDPLUS.** The scoped `UID EXPUNGE <set>` form, which
  removes only named UIDs, is only available under the `UIDPLUS` capability.

## Decision

**Every relocation is reversible; only Empty Trash removes mail from the
server.** Move, archive, and delete-to-Trash relocate a message (delete-to-Trash
moves it into the account's `\Trash` folder); none issues a server delete or
expunge. They apply optimistically to local storage and enqueue a `move`/`delete`
pending op that replays as a server **move**, and each offers an 8-second Undo
that enqueues a compensating move. Copy duplicates without touching the source.

Server-side removal lives in exactly one place: the `expunge_mailbox` trait
method, called only by the `empty_trash` command, which is gated on
`role == Trash`. No other command can reach it.

**`expunge_mailbox` confines the purge to the selected mailbox.** The IMAP
adapter (`message_ops::empty_selected`) always `SELECT`s the target mailbox
first, then:

- under `UIDPLUS`: `UID SEARCH ALL` → `UID STORE <set> +FLAGS \Deleted` →
  `UID EXPUNGE <set>`, scoping the removal to exactly the Trash UIDs; and
- without `UIDPLUS`: `UID STORE <set> +FLAGS \Deleted` → a plain `EXPUNGE`.

The plain `EXPUNGE` is sound **because** the adapter selected Trash first:
`EXPUNGE` only ever affects the selected mailbox, so it can never remove a
`\Deleted` tombstone left in the Inbox (or any other folder) by a move fallback.
This is the inverse of the per-message move policy (`message_ops::move_to`),
where a plain `EXPUNGE` is *never* issued because unrelated `\Deleted` messages
in the selected mailbox could be caught; here the mailbox *is* Trash and
emptying all of it is the intent. Scripted-server tests assert both the UIDPLUS `UID EXPUNGE` and the
non-UIDPLUS plain `EXPUNGE`-after-`SELECT Trash`, and that an empty mailbox
issues neither a `STORE` nor an `EXPUNGE`.

**A typed confirmation gates Empty Trash.** The UI requires the user to type the
word `empty` before the command runs; the button stays disabled otherwise. This
makes an irreversible, user-initiated purge deliberate rather than a stray click.

**Local cleanup follows the server, keyed by message id.** After the server
expunge succeeds (POP3/local accounts skip it — no server Trash), the command
deletes the Trash rows (recipients, FTS content, tags, and pending ops cascade)
and removes their out-of-database raw `.eml` bodies. **Compact** reclaims any
orphaned raw blob — a `.eml` whose message id has no row — closing the Wave-1
orphan-leak that a crash between row-delete and blob-removal can otherwise open.

**No new schema.** Copy reuses the existing `move` pending-op kind with a
`copy: true` payload discriminator (and the move/delete payloads gain the
`source` mailbox so the replay selects where the message physically sits after
the optimistic local move), staying inside the shipped `pending_ops.kind` CHECK
domain. Migration `0014` remains unused.

## Consequences

- Server-side removal has a single, auditable entry point (`empty_trash` →
  `expunge_mailbox`), which a security review can inspect in isolation.
- A move on a `MOVE`-less server can leave a `\Deleted` tombstone in the source
  until a later expunge/compaction reclaims it; leaving a tombstone is safe,
  silently deleting unrelated mail is not.
- Copy is at-least-once on transient-error retry (a re-`COPY` can duplicate in
  the target) and is deliberately **not** undoable via the M2 Undo.
- "Empty Trash on exit" is a per-account setting that runs the same command at
  app close, time-bounded so a slow or dead server connection cannot wedge
  shutdown.
