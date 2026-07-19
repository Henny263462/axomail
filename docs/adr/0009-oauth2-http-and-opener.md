# 0009. OAuth2 / XOAUTH2: HTTP client, system browser, and token storage

Date: 2026-07-19
Status: accepted

## Context

Thunderbird parity requires OAuth2 sign-in for the major providers (Gmail,
Microsoft 365), which no longer allow password (basic-auth) access to IMAP
and SMTP. This needs three capabilities the client did not have:

1. an HTTPS client to call the provider token endpoint (code exchange and
   refresh);
2. a way to open the provider consent page in the user's system browser;
3. a place to keep access and refresh tokens.

Constraints: MIT-compatible licenses only; a minimal dependency graph; no
weakening of the TLS trust model (ADR-0006); and no secret material in the
database or in logs (ADR-0001, ADR-0003). axomail ships **no** registered
OAuth client id, so the flow must work as a public client with a
user-supplied client id and no client secret.

## Decision

**Authorization flow — RFC 8252 loopback with PKCE.** The desktop app runs
the authorization-code flow with PKCE (`S256`). A one-shot HTTP/1.1 server
binds `127.0.0.1` on an ephemeral port and receives exactly one redirect; the
`state` parameter is verified against the value we sent (CSRF defence). PKCE
means no client secret is stored anywhere. Only `127.0.0.1` is ever bound.

**HTTP client — reqwest with `rustls-no-provider`.** `reqwest 0.13.4` is added
with `default-features = false` and features `rustls-no-provider`, `json`, and
`form`. This reuses rustls (no native-tls, no aws-lc-rs) and defers the crypto
provider to a process-wide default. The ring provider is installed once at
startup (`rustls::crypto::ring::default_provider().install_default()`),
defensively and idempotently, so TLS works regardless of call order. Token
endpoint error bodies are never surfaced in a `MailError` — only a redacted
classification crosses the boundary.

**System browser — `tauri-plugin-opener` scoped to https.** The consent URL is
opened via the opener plugin. The desktop capability set grants only
`opener:allow-open-url` scoped to `https://*`, so the app can open provider
consent pages and nothing else.

**PKCE + randomness — ring.** The PKCE verifier and the CSRF `state` come from
ring's CSPRNG, and the `S256` challenge is `base64url(sha256(verifier))` using
ring's SHA-256. ring is already the rustls crypto provider, so this adds no new
graph dependency.

**Token storage — the OS keychain, never the database.** Migration 0007 adds
`auth_mechanism` plus the **non-secret** OAuth columns (`oauth_client_id`,
`oauth_auth_url`, `oauth_token_url`, `oauth_scopes`) to `accounts`. The access
token, refresh token, and absolute expiry are serialized to JSON and stored in
the OS keychain under the account's existing `account/<id>/<direction>`
credential references (the same references a password account uses). The
payload type redacts both tokens in its `Debug` impl.

**Adapter authentication — XOAUTH2 SASL.** A transient
`ConnectionCredential` (`Password` | `XOauth2`) is threaded to the protocol
adapters. IMAP uses `AUTHENTICATE XOAUTH2` via async-imap's `authenticate`;
SMTP uses `mail_send::Credentials::new_xoauth2`; POP3 uses `AUTH XOAUTH2`
(initial-response form) in the hand-rolled adapter. The SASL string builder is
a pure helper in `mail-core`.

**Credential resolution.** At connect time the app branches on the account's
mechanism: password accounts read the keychain password; OAuth accounts load
the token set, refresh it via the token endpoint when it is expired or within a
clock-skew margin, persist any rotated refresh token, and present a fresh
access token.

**Account-first setup.** Because tokens are secret and must not cross to the
frontend, an OAuth account is created first (row only, no secret), and a
separate `oauth_authorize` command runs the interactive browser flow and stores
the tokens. Only a status (`authorized` / `denied` / `timedOut`) is returned.

## Consequences

- No client secret exists anywhere; the user registers their own OAuth app and
  supplies only a public client id (documented in the setup UI).
- reqwest pulls in the hyper/tower stack, enlarging the dependency graph. It is
  confined to the desktop crate; the protocol crates stay HTTP-free.
- The token endpoint is reached over the platform trust store
  (rustls-platform-verifier); IMAP/SMTP/POP3 trust remains the webpki bundle
  from ADR-0006, unchanged.
- POP3 XOAUTH2 depends on the server advertising SASL; when `CAPA` is known and
  omits it, the adapter returns a clear `UnsupportedCapability` rather than a
  blind attempt.
