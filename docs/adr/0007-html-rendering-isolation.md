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

## Amendment — inline `cid:` image resolution (2026-07-19, Wave 3 / Track 3.6)

Layer 1 of the decision above always envisaged inline `cid:` images being
"resolved locally". Wave 3 implements that resolution, and this amendment
records the exact mechanism because it is security-sensitive.

**Mechanism.** A `multipart/related` message references inline images from the
HTML body by `cid:` URL, carrying the bytes in sibling MIME parts. The message
detail builder collects those parts into an `InlineImages` map (content-id →
pre-encoded `data:` URI) and passes it to
`sanitize_message_html_with_inline`. The sanitizer's ammonia `attribute_filter`,
**scoped to `img@src` only**, rewrites a surviving `cid:` reference into that
part's `data:` URI. ammonia does not re-scheme-check the value a filter returns,
so the `data:` URI is emitted for the image without `data:` being an allowed
scheme anywhere.

**`data:` stays out of the scheme allowlist.** `data:` is deliberately **not**
in `Builder::url_schemes`. ammonia's scheme gate runs *before* the attribute
filter, so every attacker-supplied `data:` URL — on `<a href>`, on a hostile
`<img src="data:…">`, or on any other attribute — is stripped before the filter
ever runs. The only `data:` in the output is the one the filter substitutes for
a resolved `cid:` on `img@src`. This is stronger than filtering `data:` after
the fact and keeps the sink to a single, audited code path.

**Bounds and allowlist.** Inline embedding is limited to a raster-image MIME
allowlist (PNG/JPEG/GIF/WebP/BMP/AVIF/ICO; **SVG excluded** as foreign markup)
and a per-image size cap; the emitted MIME type is always one of those literals,
never an attacker-controlled string. A `cid:` that points at no part in the
message (or an oversize/disallowed part) resolves to nothing and renders as a
broken image — it can neither fetch nor exfiltrate.

**Counting semantics.** Because `data:` is now rejected at the scheme gate
rather than tallied by the filter, `SanitizedHtml::remote_blocked` counts the
resources that would trigger a network fetch (remote `http(s)` images and
`srcset`); inline `data:` images (attacker-supplied ones dropped, message-local
ones embedded) are not counted.

CSP and the sandboxed iframe are unchanged: `data:` images render inside the
same null-origin, script-less `srcdoc` document. Adversarial tests
(`crates/mail-security/tests/inline_cid.rs`) prove `data:` is rejected on
`<a href>` and non-`img` contexts, that a `cid:` outside the message is dropped,
and that oversize/non-image parts are not embedded.
