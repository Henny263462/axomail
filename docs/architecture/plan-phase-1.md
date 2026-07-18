# Phase 1 plan — Foundation

Phase 1 delivers the project skeleton: workspace, empty runnable app, core
models, storage, keychain abstraction, and the shell UI. No mail protocol
traffic, no fake sync implementations.

## Work packages

### 1a — Scaffold ✅ (this state)

Monorepo (Cargo + pnpm workspaces), runnable empty Tauri 2 app with Vue 3
and TypeScript strict mode, nine empty-but-documented mail crates, CI base
pipeline, license policy (deny.toml + npm license check), file-size-limit
check, LICENSE, README, first ADRs.

### 1b — Core models and errors (next)

1. Typed IDs (`AccountId`, `MailboxId`, `MessageId`, `RemoteMessageId`, …)
   as newtypes in `mail-core`; no bare `String` IDs.
2. Central `MailError` hierarchy with `thiserror`: network, DNS, TLS,
   auth, protocol, rate-limit, timeout, parsing, database, keychain,
   filesystem, unsupported-capability; each classified temporary vs.
   permanent vs. user-configuration.
3. First domain models: account, incoming/outgoing server config (without
   secrets), mailbox, message envelope.
4. Provider trait definitions (`IncomingMailProvider`,
   `OutgoingMailProvider`) — native `async fn` in traits if object safety
   allows, otherwise `async-trait`; decision recorded as an ADR.
5. Unit tests for IDs, error classification, and model invariants.
6. ADRs: error strategy, async-trait decision, workspace layout notes.

Exit criteria: `cargo test --workspace` green; no protocol code; no
storage code; public APIs documented.

### 1c — Storage and keychain

SQLite base schema with SQLx migrations (accounts, mailboxes, messages,
locations, flags, sync state, settings), `mail-storage` query surface,
keychain abstraction in `mail-security` with Windows/macOS/Linux backends,
tests (including migration round-trips), storage ADR.

### 1d — Shell UI

Design tokens (Vercel-like white mode, dark-mode token set prepared),
three-column layout with stacked navigation on small windows, minimal safe
Tauri commands, `dev-fixtures` feature flag (compile-time excluded from
release) for UI sample data, CONTRIBUTING and SECURITY documents, phase 2
step list.

## Out of scope for phase 1

Any real server communication, sync scheduling, MIME parsing, HTML
rendering, OAuth, IDLE — see the project brief's phase 2–4 boundaries.
