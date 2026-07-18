# 0002. Core dependency selection

Date: 2026-07-18
Status: accepted

## Context

Project conventions require: MIT-compatible licenses only (MIT, Apache-2.0,
BSD, ISC, MPL-2.0), minimal runtime dependencies, and a verification of
maintenance status, security posture, and version currency before adopting
a library. Versions below were verified against the crates.io and npm
registries on **2026-07-18**. Maintenance/advisory status is checked in CI
via `cargo audit`, `cargo deny`, and `pnpm audit` rather than asserted here.

## Decision

### Rust (adopted in 1a)

| Crate | Version | License | Rationale |
| ----- | ------- | ------- | --------- |
| tauri | 2.11.5 | MIT/Apache-2.0 | Application shell; current stable 2.x |
| tauri-build | 2.6.3 | MIT/Apache-2.0 | Build-time context generation |

### Rust (planned, adopted in later work packages — re-verify at adoption)

| Crate | Checked version | Purpose | Notes |
| ----- | --------------- | ------- | ----- |
| tokio | 1.53.0 | Async runtime | 1b |
| thiserror | 2.0.18 | Error derive | 1b |
| serde / serde_json | 1.0.228 / 1.x | Command/event serialization | 1b |
| sqlx | 0.9.0 | SQLite + migrations | 1c; SQLite driver, no TLS features |
| keyring | 4.1.5 | OS keychain access | 1c; verify Linux Secret Service backend quality |
| tracing | 0.1.44 | Structured logging | 1b/1c |
| rustls | latest at adoption | TLS | Phase 2, via the chosen IMAP/SMTP stack |

### Frontend (adopted in 1a)

| Package | Version | Rationale |
| ------- | ------- | --------- |
| vue | 3.5.40 | Current stable Vue 3 |
| vite | 8.1.5 | Current stable major; supported by plugin-vue 6 and vitest 4 |
| typescript | **6.0.3 (pinned `~6.0.3`)** | See below |
| vue-tsc | 3.3.7 | Typechecking for SFCs; peer `typescript >=5` |
| @vitejs/plugin-vue | 6.0.8 | Supports Vite 8 (peer `^5\|\|^6\|\|^7\|\|^8`) |
| @tauri-apps/api / cli | 2.11.1 / 2.11.4 | Matches tauri 2.11 |
| eslint | 10.7.0 | Current stable major |
| eslint-plugin-vue | 10.9.2 | Supports ESLint 10 |
| typescript-eslint | 8.64.0 | Supports ESLint 10; TS peer `>=4.8.4 <6.1.0` |
| vitest | 4.1.10 | Supports Vite 8 |
| @vue/test-utils | 2.4.11 | Component mounting in tests |
| happy-dom | 20.10.6 | Lightweight DOM for tests |
| @eslint/js | 10.0.1 | Core JS recommended rules for the flat config |
| @types/node | 24.13.3 | Node typings for vite.config.ts / tooling typecheck |

**TypeScript 6.0.3, not 7.x.** npm `latest` is TypeScript 7.0.2 (the native
compiler). typescript-eslint 8.64.0 constrains its TypeScript peer to
`>=4.8.4 <6.1.0`, so 7.x (and 6.1+) would break linting. 6.0.3 is the
newest release inside the supported range; vue-tsc accepts it. Revisit when
typescript-eslint supports TS 7.

**Not adopted.** No UI component library, no CSS framework, no state
library yet (Pinia only when global UI state genuinely appears), no HTTP
client, no Electron/Chromium components.

## Consequences

- All adopted licenses are MIT/Apache-2.0/ISC-family; `deny.toml` and
  `scripts/check-npm-licenses.mjs` enforce the policy in CI.
- Planned crates (sqlx, keyring, rustls, the IMAP/POP3/SMTP protocol
  stack) are **to be re-verified** — registry data, maintenance, and
  advisories — in the work package that adopts them, each with its own ADR
  where the choice is non-obvious (protocol stack, keychain backends).
- Pinning TypeScript with `~` (patch-only) is deliberate; a minor bump to
  6.1+ would silently leave typescript-eslint's supported range.
