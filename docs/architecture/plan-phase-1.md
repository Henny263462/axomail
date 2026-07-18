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

### 1b — Core models and errors ✅

Delivered 2026-07-18: typed IDs as newtypes (`AccountId`, `MailboxId`,
`MessageId`, `AttachmentId`, `CredentialRef`, `RemoteMessageId`),
central `MailError` hierarchy with classification
(temporary/permanent/user-configuration, ADR-0003), domain models
(account and server configs without secrets, mailbox with role
detection, message envelope), provider traits with native `async fn`
and enum-dispatch strategy (ADR-0004), 20 unit tests.

### UI shell (pulled forward from 1d) ✅

Delivered 2026-07-18 on owner request: design tokens (white mode plus
prepared dark token set), three-column shell with stacked navigation
below 880 px, Sidebar/MessageList/MessageView panes with honest empty
states, `dev-fixtures` sample data strictly behind a compile-time flag
(verified absent from release bundles), component tests, and the
browser-served web preview (ADR-0005). Still open from 1d: real Tauri
command surface, CONTRIBUTING, SECURITY.

### 1c — Storage and keychain (next)

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
