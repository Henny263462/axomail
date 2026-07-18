# 0007. HTML mail rendering isolation

Date: 2026-07-18
Status: accepted

## Context

Rendering HTML mail is the most security-critical UI decision (project
brief §11): message content is untrusted input rendered inside a
WebView that also hosts the privileged application UI. Options
considered:

1. **Rust-side sanitization + sandboxed `iframe srcdoc`** in the main
   WebView.
2. A second, isolated Tauri WebView window per message.
3. Converting HTML to an internal rich-text model (lossy).

## Decision

Option 1: defense in depth with four layers.

1. **Sanitize in Rust** (`axomail-security`, ammonia): allowlist-based.
   Strips scripts, event handlers, forms, iframes/objects/embeds,
   `javascript:`/`data:` URLs (except embedded `cid:` images resolved
   locally), and style properties that can escape the viewport
   (`position:fixed`, etc.). External resource URLs (`http(s)` images,
   fonts, css) are rewritten to inert placeholders by default —
   blocking trackers; a per-message user action can re-allow images
   later (phase 4+).
2. **Sandboxed iframe**: the sanitized HTML is rendered via
   `<iframe sandbox srcdoc=...>` — no `allow-scripts`, no
   `allow-same-origin`, so even a sanitizer bypass lands in a
   null-origin, script-less document with no access to Tauri IPC.
3. **CSP** (tauri.conf.json) stays restrictive; `frame-src` is not
   opened to remote origins; the iframe content inherits the app CSP,
   which blocks external loads as a third net.
4. **Link policy**: clicks inside the message document are intercepted
   (sandbox blocks navigation; link activation is surfaced to the
   shell), the real target URL is shown, and opening happens only
   through an audited Rust command (phase 2 command surface).

A plain-text view is always available and is the default for messages
without an HTML part.

## Consequences

- Two independent barriers (sanitizer, sandbox) must both fail before
  script execution is possible, and a third (CSP) before exfiltration.
- `srcdoc` requires care with size (very large messages are truncated
  with a "show full message" action later).
- No second WebView keeps memory low (§ priorities) and avoids
  multi-window IPC complexity.
- Threat model documented in SECURITY.md; sanitizer gets adversarial
  unit tests (script/style/URL smuggling corpora).
