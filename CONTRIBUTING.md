# Contributing to axomail

Thanks for your interest. This document covers the practical rules; the
architecture rules live in [docs/architecture/](docs/architecture/) and
the ADRs in [docs/adr/](docs/adr/).

## Setup

- Rust ≥ 1.95 (MSRV, see `rust-version` in `Cargo.toml`)
- Node.js ≥ 22.12, pnpm ≥ 11
- Linux: WebKitGTK 4.1 dev packages (see Tauri prerequisites)

```sh
pnpm install
pnpm tauri dev
```

## Quality gates (all must pass before a PR)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
pnpm typecheck && pnpm lint && pnpm test && pnpm build
pnpm check:file-sizes
```

CI enforces these plus `cargo deny`, `cargo audit`, `pnpm audit`, the
npm license check, and a grep that fails if dev-fixtures leak into the
production bundle. Merges with failing checks or warnings are not
allowed.

## Code rules

- **File size:** guideline 300 non-blank lines, hard ceiling 400
  (tests exempt). Split along responsibilities before growing past the
  guideline. Enforced by ESLint `max-lines` and
  `scripts/check-file-sizes.mjs`.
- **Language:** code, comments, commits, and docs are English. UI
  strings are English (i18n-ready later).
- **Rust:** no `unwrap()`/`expect()` in production paths (clippy
  escalates them); errors go through `axomail_core::MailError`; no
  protocol types outside their adapter crate; `#![forbid(unsafe_code)]`
  in mail crates.
- **Vue/TS:** strict mode; one component per file; extract logic into
  composables; no mail bodies, raw MIME, or credentials in frontend
  state; every backend call goes through `lib/tauri.ts`.
- **No secrets anywhere** but the OS keychain — not in SQLite, logs,
  errors, fixtures, or tests.
- **No placeholder implementations that look finished.** Honest empty
  states over fake data; `dev-fixtures` only behind the compile-time
  flag.

## Commits and PRs

- Small, verifiable steps; each commit leaves the tree green.
- Commit messages in English, imperative subject, context in the body.
- Significant architecture decisions get an ADR (`docs/adr/`, next free
  number) in the same PR.

## Tests

New logic ships with tests: Rust unit tests next to the module,
integration tests under `tests/`, frontend tests in
`apps/desktop/tests/`. Never use real private mail accounts in
automated tests; local test servers live in `crates/mail-test-support`.
