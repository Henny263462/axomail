# 0001. Protocol and storage architecture

Date: 2026-07-18
Status: accepted

## Context

axomail must support IMAP (two-way sync), POP3 (download-only), and SMTP
(submission) behind a UI that starts instantly from local data. The binding
priority order is: correctness/data-loss avoidance, security, speed, low
memory, offline capability, usability, features. The frontend must never
touch protocols, SQL, raw MIME, or credentials.

## Decision

**Layering.** Three strictly separated layers:

1. **Vue frontend** (`apps/desktop/src`) — rendering, navigation, transient
   UI state. Talks to the backend only through typed Tauri commands and
   events.
2. **Tauri shell** (`apps/desktop/src-tauri`) — thin composition layer:
   command definitions, event emission, capability configuration. No mail
   logic.
3. **Rust mail core** (`crates/`) — all protocol, storage, parsing,
   security, and sync logic, split into focused crates.

**Protocol abstraction.** `mail-core` defines the provider traits
(`IncomingMailProvider`, `OutgoingMailProvider`) plus typed IDs, domain
models, and the central `MailError` hierarchy. `mail-imap`, `mail-pop3`,
and `mail-smtp` are adapters behind those traits; protocol-specific types
must not leak out of their adapter crate. The sync engine (`mail-sync`)
and storage (`mail-storage`) depend only on `mail-core` abstractions,
never on a concrete protocol crate.

**Source-of-truth semantics.**

- IMAP: the server is authoritative for server-side folders and flags; the
  local store is an offline cache keyed by mailbox + UID, with
  `UIDVALIDITY`/`UIDNEXT` tracked per mailbox.
- POP3: download-only source; local folders and flags are authoritative.
  UIDL-based dedup; deletion on the server only via an explicit, doubly
  guarded user policy.
- SMTP: submission only; drafts and outbox live locally.

**Storage.** SQLite via SQLx with versioned migrations, owned entirely by
`mail-storage`. Metadata and state in SQLite; attachments as files in a
controlled app-data directory (atomic writes, integrity hashes); raw
messages stored only where offline use or re-parsing requires it.
Credentials are never stored in SQLite — only non-secret credential
references pointing into the OS keychain (handled by `mail-security`).
SQLite FTS5 is the default assumption for local search; suitability will
be confirmed in a dedicated ADR before phase 4.

## Consequences

- Protocol libraries are swappable without touching sync, storage, or UI.
- The trait boundary adds indirection, but enables local test servers
  (`mail-test-support`) to drive the full sync path in integration tests.
- IMAP IDLE/CONDSTORE/QRESYNC and OAuth must fit the trait design later;
  the traits therefore model capabilities explicitly rather than assuming
  a fixed feature set.
- Package names carry the `axomail-` prefix (directory names keep the
  `mail-*` layout) so internal crates never clash with public crates.io
  names such as `mail-parser`.
