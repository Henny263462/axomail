# axomail

[![CI](https://github.com/Henny263462/axomail/actions/workflows/ci.yml/badge.svg)](https://github.com/Henny263462/axomail/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Henny263462/axomail?include_prereleases)](https://github.com/Henny263462/axomail/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A fast, lightweight, open-source desktop mail client for Windows, macOS, and
Linux. Built with Tauri 2, Rust, and Vue 3.

> **Status: pre-alpha.** A broad desktop-mail feature set is implemented —
> two-way IMAP sync (CONDSTORE/QRESYNC) with IDLE push, POP3, and SMTP;
> OAuth2/XOAUTH2 (Gmail, Microsoft 365) and password auth; multiple
> accounts, identities, folder management, tags, and a unified inbox;
> reply/forward with a rich-text composer, attachments, and server-side
> Sent copies; a virtualized threaded list with quick filters, full-text
> search, and saved searches; move/copy/archive/Trash with undo; an
> address book, message filters, a Bayesian junk filter, iCalendar
> invitations, OpenPGP, an offline store, a command palette, mbox
> import/export, printing, and English + German localization — but it has
> not yet been hardened against real-world servers at scale. Expect rough
> edges; release bundles are unsigned. See
> [docs/architecture/](docs/architecture/) for the roadmap and
> [Releases](https://github.com/Henny263462/axomail/releases) for downloads.

## Principles

Priorities, in binding order:

1. Correctness and data-loss avoidance
2. Security
3. Speed
4. Low memory usage
5. Offline capability
6. Usability
7. Feature scope

axomail is desktop-first and private by design: no cloud service, no
mandatory account, no telemetry, no ads. All mail data, settings, and search
indexes are stored locally. Credentials will live exclusively in the
operating system keychain — never in the database or the frontend (keychain
integration lands in work package 1c; the current scaffold accepts and
stores no credentials at all).

## Tech stack

- [Tauri 2](https://tauri.app) — application shell (no Electron/Chromium bundle)
- Rust (stable) — mail core: IMAP, POP3, SMTP, sync, storage, parsing, security
- Vue 3 + TypeScript (strict) + Vite — UI only; no protocol, SQL, or MIME logic
- SQLite (via SQLx) — local metadata and state
- Tokio — async runtime

## Repository layout

```
apps/desktop/       Tauri 2 + Vue 3 desktop application
crates/mail-core/       Domain models, typed IDs, provider traits, errors
crates/mail-imap/       IMAP adapter (two-way sync)
crates/mail-pop3/       POP3 adapter (download-only)
crates/mail-smtp/       SMTP adapter (submission)
crates/mail-sync/       Protocol-agnostic sync engine
crates/mail-storage/    SQLite schema, migrations, attachment store
crates/mail-parser/     MIME parsing
crates/mail-security/   HTML sanitization, keychain, redaction
crates/mail-test-support/  Test fixtures and local protocol test servers
docs/               Architecture, ADRs, performance notes, protocol notes
```

## Development

Prerequisites:

- Rust ≥ 1.95 (stable; this is the MSRV, see `rust-version` in `Cargo.toml`)
- Node.js ≥ 22.12
- pnpm ≥ 11 (`corepack enable` or standalone install)
- Linux only: WebKitGTK 4.1 development packages
  (see the [Tauri prerequisites](https://tauri.app/start/prerequisites/))

```sh
pnpm install        # install frontend dependencies
pnpm tauri dev      # run the desktop app in development mode
pnpm tauri build    # build a release bundle
pnpm dev:fixtures   # UI development with sample data (dev-fixtures flag)
```

### Web preview

The same frontend runs as a browser-served preview (ADR-0005): full UI
shell and design system, but no mail features — browsers cannot speak
IMAP/POP3/SMTP or access the OS keychain, so those stay desktop-only.

```sh
pnpm dev:web        # serve the preview locally
pnpm build:web      # build the static preview into apps/desktop/dist
```

Quality checks:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck
pnpm lint
pnpm test
pnpm check:file-sizes
```

## Documentation

- [Architecture overview](docs/architecture/overview.md)
- [Architecture decision records](docs/adr/)
- [Open decisions and risks](docs/architecture/open-decisions.md)
- [Performance goals and measurements](docs/performance/)

## License

[MIT](LICENSE)
