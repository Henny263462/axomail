# Phases 2–4 delivery plan

Continuation of [plan-phase-1.md](plan-phase-1.md). Decisions backing
this plan: ADR-0006 (protocol stack), ADR-0007 (HTML rendering
isolation), ADR-0008 (FTS5 search).

## Phase 2 — one account, IMAP and SMTP

**Stage A (protocol crates, parallel):**
`axomail-parser` (MIME → internal model via the external mail-parser),
`axomail-security` HTML sanitization (ammonia, ADR-0007 policy),
`axomail-imap` (async-imap adapter: TLS/STARTTLS, capabilities,
SPECIAL-USE discovery, UID-based incremental sync with UIDVALIDITY
handling, flags/move/delete, timeouts), `axomail-smtp` (mail-send +
mail-builder: alternative bodies, attachments, Bcc only as RCPT),
`axomail-sync` (job engine: persistent cancellable jobs, bounded
parallelism, backoff with jitter, crash recovery, progress events)
plus sync-state/ingest repositories in `axomail-storage`, and
`axomail-test-support` (scripted IMAP/SMTP/POP3 servers with local
TLS identities).

**Stage B (wiring, parallel):** Tauri commands (connection test,
manual sync, full message fetch → parse → sanitize, send with
draft/outbox flow, sync events forwarded to the frontend) and the
frontend (message detail with sandboxed-iframe HTML + plain-text
toggle, composer, drafts/outbox views, sync status, error details,
link-target confirmation).

## Phase 3 — POP3

Hand-rolled adapter on tokio + rustls (ADR-0006): CAPA/STAT/LIST/
UIDL/RETR with TOP when available, UIDL-based dedup with a
missing-UIDL fallback, download-only semantics, no server deletion
without the doubly guarded opt-in policy. Wired into the same engine
via the composition-root dispatch enum.

## Phase 4 — multiple accounts and offline use

Unified inbox, background sync scheduling, desktop notifications
(tauri-plugin-notification — requires a capability addition, reviewed
against the minimal-permissions rule), FTS5 search (ADR-0008) with a
search UI, robust offline outbox with resume, and job recovery after
restart. Performance measurement scripts land here alongside real
data paths (docs/performance/ stays free of unmeasured claims).

## Review cadence

After each stage: full local verification (fmt, clippy -D warnings,
tests, frontend suite, deny/audit, file-size limits), a multi-agent
review workflow with adversarial verification, and a codex CLI
second-opinion pass; confirmed findings are fixed before the commit.
