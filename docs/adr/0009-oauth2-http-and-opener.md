# 0009. OAuth2 HTTP client and system-browser opener

Date: 2026-07-19
Status: accepted

## Context

axomail cannot connect to Gmail or Microsoft 365 today: both require OAuth2
bearer tokens presented through the XOAUTH2 SASL mechanism, and neither
password auth (Google) nor basic auth (Microsoft) is available for those
providers anymore. Wave 1 Track 1.2 (cluster E3) implements that auth path.
Two capabilities the current stack lacks are prerequisites and are added here
in Wave 1 Stage 0, ahead of the fan-out, so the dependency and capability
surface is reviewed once:

1. **An HTTPS client.** The OAuth2 authorization-code flow needs a token
   exchange and refresh against the provider's token endpoint
   (`https://oauth2.googleapis.com/token`,
   `https://login.microsoftonline.com/…/oauth2/v2.0/token`), returning JSON.
   The workspace has TLS for IMAP/SMTP/POP3 (rustls 0.23 + `ring` via
   `async-imap`, `mail-send`, and the hand-rolled POP3 client) but no
   general-purpose HTTP client. The same client later serves ISPDB /
   Thunderbird autoconfig lookups (Track 5.3), which are HTTPS-only.

2. **A way to open the system browser.** RFC 8252 (OAuth 2.0 for Native Apps)
   requires the authorization request to run in the user's system browser, not
   an embedded webview. Google and Microsoft actively block embedded-webview
   user agents for OAuth, so reusing the Tauri webview is both non-compliant
   and non-functional. The app must hand the consent URL to the OS default
   browser and receive the authorization code back on a loopback redirect.

Project rules from ADR-0002 / ADR-0006 apply: MIT-compatible licenses only,
minimal dependency count, rustls (no native-tls), no second TLS/crypto stack,
and versions verified against crates.io at adoption. Credentials never enter
SQLite (ADR-0001) — only OS-keychain references.

## Decision

### HTTPS client: `reqwest` 0.13.4, rustls + ring, `json`

```toml
reqwest = { version = "0.13.4", default-features = false,
            features = ["rustls-no-provider", "json"] }
```

- **`default-features = false`** drops reqwest's default `default-tls`
  (which now resolves to rustls with the **aws-lc-rs** provider) plus
  `charset`, `http2`, and `system-proxy`.
- **`rustls-no-provider`** enables rustls/`hyper-rustls`/`tokio-rustls`
  *without* selecting a crypto provider. The process installs `ring` as the
  default `rustls::crypto::CryptoProvider` at startup (Track 1.2), so reqwest
  reuses the **exact rustls 0.23.42 + ring 0.17.14** already compiled for
  IMAP/SMTP/POP3. Cargo feature-unifies the single `rustls` crate (our
  workspace dep contributes the `ring` feature); no aws-lc-rs and no
  native-tls/OpenSSL enter the graph. The only genuinely new crate is
  `hyper-rustls`. reqwest 0.13's rustls features route certificate
  verification through `rustls-platform-verifier`, which is already in the
  tree — `mail-send` (SMTP) pulls the same crate — so root/trust handling is
  identical to the existing outgoing-mail stack.
- **`0.13.4`** is the version tauri already resolves transitively, so pinning
  it keeps a single reqwest in `Cargo.lock` (no duplicate-version churn under
  `deny.toml`'s `multiple-versions` check) while feature-unification simply
  adds `rustls-no-provider` + `json` to that shared copy.
- **`json`** covers deserializing the token response. The token *request* body
  is `application/x-www-form-urlencoded` (RFC 6749 §4.1.3); Track 1.2 either
  encodes that body directly or opts into reqwest's `form` feature — it is
  deliberately not enabled here to keep the surface minimal.

Alternatives considered:

- **`reqwest` with the plain `rustls` feature** — pulls aws-lc-rs (a second C
  crypto library) *and* still pulls rustls-platform-verifier. Rejected: a
  second crypto backend is exactly the duplication ADR-0006 forbids.
- **`reqwest` with `native-tls`** — adds OpenSSL/SChannel/Secure Transport per
  platform. Rejected: contradicts the rustls-everywhere decision (ADR-0006).
- **`ureq`** (blocking, rustls) — smaller, but blocking; the token exchange
  runs inside the Tokio-based command/sync layer, so an async client avoids a
  `spawn_blocking` bridge. `ureq` also does not share rustls-platform-verifier
  with the existing stack.
- **Hand-rolled hyper client** — reqwest *is* the thin ergonomic layer over
  hyper we would otherwise re-implement (redirects, proxy env, JSON); hyper is
  already in the tree via tauri, so reqwest adds little.

### XOAUTH2 encoding: `base64` 0.22.1

The XOAUTH2 SASL initial-client-response is
`base64("user=<addr>\x01auth=Bearer <token>\x01\x01")`. `base64` 0.22.1 (already
present transitively; now a first-class, cargo-deny-vetted workspace dep) builds
it for IMAP `AUTHENTICATE XOAUTH2`. SMTP goes through `mail-send`'s
`Credentials::new_xoauth2`, which encodes internally.

### System-browser open: `tauri-plugin-opener` 2.5.4, scoped to `https://*`

The official Tauri 2 plugin (`Apache-2.0 OR MIT`, requires `tauri >= 2.10`,
compatible with our 2.11.5) is registered with `tauri_plugin_opener::init()`.
The capability grant in `capabilities/default.json` is the **minimum** that
opens the consent screen and nothing else:

```json
{ "identifier": "opener:allow-open-url", "allow": [{ "url": "https://*" }] }
```

`opener:allow-open-url` enables only the `open_url` command; the plugin
enforces the scope in `open_url` (`is_url_allowed` denies any URL without a
matching allow entry). The single `https://*` entry (glob match, path-crossing)
permits opening any HTTPS consent URL in the default browser and rejects
everything else. We deliberately do **not** grant:

- `opener:allow-open-path` (arbitrary local-file/URL opening),
- `opener:allow-reveal-item-in-dir` (file-manager reveal),
- `opener:allow-default-urls` / `opener:default` (which also allow `mailto:`,
  `tel:`, and plain `http://`).

The `app` field defaults to "default application", so `open_url(url, None)`
opens the user's default browser (the RFC 8252 requirement); no specific
browser binary is whitelisted.

### Loopback redirect (outline, implemented in Track 1.2)

1. Bind a `tokio::net::TcpListener` on `127.0.0.1:0` (ephemeral port); the
   redirect URI is `http://127.0.0.1:<port>/` (loopback, per RFC 8252 §7.3 —
   not registered in this ADR's scope).
2. Build the authorization URL with PKCE (`code_challenge`) and a `state` nonce
   and open it via `open_url` in the system browser.
3. The listener accepts one request, validates `state`, extracts `code`.
4. Exchange `code` + `code_verifier` at the token endpoint via `reqwest`
   (backend-side HTTPS), receive access/refresh tokens as JSON.
5. Persist **only** in the OS keychain via `KeyringSecretStore` (ADR-0001); the
   database stores non-secret client config and a keychain reference. Refresh
   uses the same reqwest client.

### CSP is unaffected

`tauri.conf.json` `connect-src` stays `ipc: http://ipc.localhost` (dev adds the
Vite origin). Every OAuth network call — the token exchange, the refresh, and
the loopback accept — happens in the Rust backend or the OS browser, never in
the webview, so no CSP origin is added. The webview never contacts a provider.
Keeping the OAuth traffic out of the webview is a security benefit, not a gap.

## Consequences

- Gmail / Microsoft 365 become reachable once Track 1.2 lands the auth-mechanism
  enum, SASL wiring, and token flow; ISPDB autoconfig (5.3) reuses the same
  reqwest client.
- New crates in `Cargo.lock`: `reqwest` gains `rustls-no-provider` + `json`
  features on the copy tauri already resolved, adding `hyper-rustls`; `base64`
  and `tauri-plugin-opener` become direct deps. No aws-lc-rs, no native-tls, no
  second TLS or crypto stack — `ring` + rustls 0.23 remain the only backend.
  All licenses are within the `deny.toml` allow-list; `cargo deny check` gates
  regressions.
- **Runtime obligation for Track 1.2:** because `rustls-no-provider` ships no
  built-in provider, the process must call
  `rustls::crypto::ring::default_provider().install_default()` once at startup
  before the first reqwest client is built, or TLS handshakes panic. This is a
  code step, intentionally not added here (Stage 0 adds no placeholder code).
- The opener grant is a reviewable capability escalation (the app could
  previously open nothing). It is scoped to HTTPS URL launches only; extending
  it (e.g. `open_path` for attachment "open with…") requires a new, separately
  reviewed capability entry and likely its own ADR.
- Front-end ergonomics: invoking the browser open from Vue can go through the
  raw IPC (`invoke('plugin:opener|open_url', { url })`) or the
  `@tauri-apps/plugin-opener` npm package; adding that JS package is left to
  Track 1.2, which owns `AccountSetupDialog.vue`.
