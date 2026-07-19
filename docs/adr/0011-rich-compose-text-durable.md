# 0011. Rich compose: session-scoped HTML and attachments over a text-only durable store

Date: 2026-07-19
Status: accepted

## Context

Wave 3 track 3.5 adds a rich-text composer: an HTML body alternative and
file attachments on send. The send machinery already supports both —
`OutgoingMessage` carries `html_body` and `attachments`, and `mail-smtp`
assembles `multipart/alternative` (+ `multipart/mixed`) via `mail-builder`
(proven by `crates/mail-smtp/tests/submission.rs`).

The gap is persistence. A composed message is durably represented twice: as a
row in the `drafts` table and, once sent, as a row in the `outbox` (the queue
that provides the duplicate-send guard, offline queueing, scheduled "send
later", and manual retry). Both tables store **plain text only** — `body_text`,
recipients, subject. Storing an HTML alternative and attachment bytes would
require new columns.

Wave 3 is explicitly a **no-migration** wave (migration `0014` is reserved and
expected to stay unused). So the durable store cannot gain HTML or attachment
columns this wave.

## Decision

Keep the durable store text-only. Treat the HTML body and attachments as
**session-scoped enrichments carried in memory** through a single immediate
send, never persisted:

- `ComposeInput` gains `html_body: Option<String>` and `attachments:
  Vec<String>` (absolute paths). Neither is written to `drafts` or `outbox`;
  `save_draft` ignores them, so a draft (and every autosave) captures the
  faithful plain-text alternative only.
- On an **immediate** send the command reads the attachment bytes (Rust
  `std::fs`, off the blocking pool), re-sanitizes the HTML with
  `mail-security::sanitize_message_html`, and threads both into the one live
  submission via a `MessageExtras` value. The outbox row it leaves behind is
  the text-only record.
- A **deferred** send — scheduled "send later" or queued while offline — cannot
  carry attachments to a later, worker-driven submission (they are not
  persisted), so a deferred send that carries attachments is **rejected up
  front** with a clear `Configuration` error rather than sent silently
  stripped. An HTML-only deferred send is accepted and degrades to its stored
  text alternative.
- A **retry** from the Outbox rebuilds from the text-only row, so it re-sends
  plain text. The composer keeps the rich content on a failed send (it stays
  open) and is the fidelity-preserving retry surface.

Attachment file IO follows ADR-0010: the webview picks paths via
`tauri-plugin-dialog` and never gains an `fs:*` scope; a Rust command reads the
bytes, sanitizes the recipient-facing filename
(`sanitize_attachment_filename`), and enforces a 25 MiB total cap.

## Consequences

- No schema change; the no-migration constraint holds.
- The common path — compose HTML with attachments and send now — is full
  fidelity, with proper MIME multipart on the wire.
- Two deliberate, documented limitations remain, each safe (no silent data
  loss) rather than lossy-by-surprise:
  1. Scheduling or offline-queueing a message **with attachments** is refused.
  2. Retrying a failed send **from the Outbox pane** re-sends plain text;
     retrying from the still-open composer preserves the rich content.
- Closing both gaps — attachments surviving a restart, and scheduled rich sends
  — is a follow-up that needs the reserved `0014` migration (an outbox/draft
  HTML column plus an attachment blob/side table). That work is intentionally
  deferred out of this wave.
- Defense in depth on the HTML body: the editor keeps output in an allowlisted
  subset client-side (mirroring ADR-0007), and the send path re-sanitizes
  authoritatively before assembly, so a compromised or bypassed webview cannot
  put active content on the wire.
