# 0006. Protocol and content library selection (phases 1c–4)

Date: 2026-07-18
Status: accepted

## Context

Phases 1c–4 need: SQLite access, OS keychain access, IMAP, SMTP, POP3,
MIME parsing and building, HTML sanitization, TLS, and structured
logging. Project rules: MIT-compatible licenses only, active
maintenance, rustls where reliably supported, minimal dependency count.
Registry data (crates.io) verified on **2026-07-18**; licenses and
last-release dates checked per crate. CI (`cargo deny`, `cargo audit`)
guards regressions.

## Decision

| Crate | Version | License | Updated | Role |
| ----- | ------- | ------- | ------- | ---- |
| sqlx | 0.9.0 | MIT OR Apache-2.0 | 2026-05 | SQLite + migrations (`runtime-tokio`, `sqlite`) |
| keyring | 4.1.5 | MIT OR Apache-2.0 | 2026-07 | OS keychain (Windows/macOS/Secret Service) |
| tokio | 1.53.0 | MIT | 2026 | async runtime |
| rustls / tokio-rustls | 0.23.42 / 0.26.4 | Apache/MIT/ISC | 2026 | TLS everywhere; no native-tls |
| webpki-roots | 1.0.9 | MPL-2.0/CDLA | 2026 | root store |
| async-imap | 0.11.3 | MIT OR Apache-2.0 | 2026-07-17 | IMAP client core |
| mail-send | 0.6.1 | Apache-2.0 OR MIT | 2026-07 | SMTP submission (rustls-based) |
| mail-parser | 0.11.5 | Apache-2.0 OR MIT | 2026-07 | MIME parsing (tolerant, widely used) |
| mail-builder | 0.4.4 | Apache-2.0 OR MIT | 2025-08 | MIME construction for the composer |
| ammonia | 4.1.3 | MIT OR Apache-2.0 | 2026-06 | HTML sanitization (html5ever-based) |
| tracing / tracing-subscriber | 0.1.44 / 0.3.23 | MIT | 2026 | structured logging |
| tokio-util | 0.7.18 | MIT | 2026 | cancellation tokens |
| tauri-plugin-notification | 2.3.3 | MIT/Apache | 2026 | desktop notifications (phase 4) |

**POP3 is hand-rolled** in `axomail-pop3` on tokio + tokio-rustls: the
async POP3 crate landscape has no actively maintained option, and the
protocol is small (a dozen line-oriented commands). This also keeps
UIDL edge-case handling fully under our control.

**Adapter isolation stands:** these crates appear only inside their
adapter crate (`async-imap` in `axomail-imap`, `mail-send` in
`axomail-smtp`, `mail-parser` in `axomail-parser`, `ammonia` in
`axomail-security`, `sqlx` in `axomail-storage`, `keyring` in
`axomail-security`). `axomail-core` stays serde+thiserror only.

## Consequences

- The external `mail-parser` crate coexists with our `axomail-parser`
  wrapper crate (directory `crates/mail-parser`); the `axomail-` package
  prefix (ADR-0002) prevents the name clash.
- webpki-roots is MPL/CDLA — inside the allowed license set; deny.toml
  already permits MPL-2.0 (CDLA-Permissive-2.0 added on adoption).
- async-imap drives the IMAP session; UIDVALIDITY bookkeeping, batching,
  and retry policy remain our code in `axomail-imap`/`axomail-sync`.
- Every version is re-checked by `cargo deny`/`cargo audit` in CI; a
  breaking maintenance change in async-imap is the highest-risk item
  and would trigger a follow-up ADR (fallback: imap-proto + own
  session layer).
