# Open decisions and risks

Living document; resolved items move into ADRs. State as of work package 1a
(2026-07-18).

## Open decisions

| # | Decision | Planned resolution |
| - | -------- | ------------------ |
| 1 | IMAP/POP3/SMTP protocol stack: dedicated crates (e.g. async-imap, mail-send family) vs. hand-rolled protocol cores on tokio + rustls. License, maintenance, and rustls compatibility must be verified per crate. | ADR before phase 2 implementation |
| 2 | Native `async fn` in traits vs. `async-trait` for the provider traits (dyn-dispatch requirement decides) | ADR in 1b |
| 3 | MIME parser: external crate (e.g. `mail-parser`, license check required) vs. own parser in `axomail-parser` | ADR before phase 2 |
| 4 | HTML rendering isolation strategy for the message view (Rust-side sanitizing + sandboxed iframe vs. separate webview vs. other) — the project's most security-critical UI decision | Dedicated ADR before the message view (phase 2) |
| 5 | Keychain backend details on Linux (Secret Service variants, headless fallback) | ADR in 1c |
| 6 | SQLite settings (WAL, synchronous level) per platform | ADR in 1c |
| 7 | FTS5 suitability for local search | ADR before phase 4 |
| 8 | Dark-mode token values (structure prepared in 1d, visual design later) | After white-mode MVP |
| 9 | GitHub Actions major versions (checkout/setup-node/pnpm actions) were chosen from known-good majors; verify against the actions' current releases when the repo goes online | First CI run on the hosted repo |

## Risks

- **Ecosystem drift (high impact):** TypeScript 7 is npm `latest` but
  typescript-eslint only supports `<6.1.0`; we pin `~6.0.3`. A similar
  constraint may hit other tools; renovate-style updates need CI gating.
- **Rust protocol crate maturity (high impact):** the async IMAP/POP3
  crate landscape is thin; a hand-rolled protocol core is a realistic
  outcome and would grow the phase 2 effort considerably.
- **CI licence/audit gates unverified on hosted runners:** cargo-deny,
  cargo-audit, and the pnpm license check run locally, but the full
  cross-OS pipeline has not executed on GitHub yet (repo not published).
- **`dist/` and pure-Rust builds:** with tauri 2.11.5, debug-profile
  `cargo check`/`clippy`/`test` work without the frontend dist directory
  (verified locally). CI still creates an empty `dist/` as a guard, since
  release-profile builds embed assets and Tauri's behavior here has
  changed between versions.
- **Windows/macOS/Linux parity:** all local verification for 1a happened
  on Windows 11; macOS and Linux builds rely on CI.
