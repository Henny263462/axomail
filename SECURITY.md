# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately via GitHub's private
vulnerability reporting on this repository once it is public. Do not
open public issues for security reports. You can expect an initial
response within 7 days.

## Supported versions

axomail is pre-release; only the latest `main` receives fixes.

## Threat model

**Assets.** Account credentials, mail content (local SQLite store, raw
messages, attachments), and the integrity of the user's system.

**Trust boundaries.**

1. **Mail content is hostile.** Every byte from a mail server (bodies,
   headers, attachment names, HTML) is untrusted input. MIME parsing is
   fuzzed-by-corpus and panic-free; HTML passes an allowlist sanitizer
   in Rust (ammonia) and renders only inside a script-less, null-origin
   sandboxed iframe with a restrictive CSP (see ADR-0007). External
   resources and tracking pixels are blocked by default. Attachment
   file names are sanitized against path traversal and reserved names
   before touching the filesystem.
2. **The frontend is unprivileged.** The WebView has minimal Tauri
   capabilities (`core:default` only), no shell or filesystem
   permissions, no remote content, and a restrictive CSP. It never
   holds credentials, raw MIME, or unsanitized HTML. All privileged
   operations are typed Tauri commands in Rust.
3. **Credentials live only in the OS keychain** (Windows Credential
   Manager, macOS Keychain, Secret Service). The database stores
   non-secret references. Logs redact addresses and secret-like
   strings; diagnostics exports are re-redacted before writing.
4. **Network.** TLS via rustls; STARTTLS upgrades happen before any
   credential is sent; certificate validation failures are surfaced,
   never silently retried, and there is no "accept insecure
   certificate" option in the MVP.

**Out of scope (MVP).** Defending against a compromised local OS
account, malware with user privileges, or a malicious mail *server*
beyond protocol-level robustness (a hostile server can withhold or
alter mail it serves; it must not be able to execute code or read
files here).

## Hardening measures in place

- `cargo audit`, `cargo deny` (licenses, advisories, sources), and
  `pnpm audit` in CI
- clippy pedantic panics policy (`unwrap_used`/`expect_used` escalated)
- `#![forbid(unsafe_code)]` in all mail crates
- dev-fixtures compile-time excluded from releases and leak-checked in CI
