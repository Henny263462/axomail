# axomail

A fast, lightweight, open-source desktop mail client for Windows, macOS, and
Linux. Built with Tauri 2, Rust, and Vue 3.

> **Status: pre-alpha.** Work package 1a (project scaffold) is complete. The
> application opens an empty shell window; no mail functionality exists yet.
> See [docs/architecture/plan-phase-1.md](docs/architecture/plan-phase-1.md)
> for the roadmap.

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
