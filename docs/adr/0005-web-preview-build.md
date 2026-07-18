# 0005. Browser-served web preview of the UI

Date: 2026-07-18
Status: accepted

## Context

The original brief excluded a web app from the initial scope. On
2026-07-18 the project owner requested a web version. A browser cannot
speak IMAP/POP3/SMTP, open raw TLS sockets, access an OS keychain, or
run the SQLite store — so a full web mail client would require a server
component, which remains out of scope.

## Decision

- The existing Vue frontend doubles as a **web preview**: the same SPA,
  built by Vite, served from any static host (`pnpm build:web`,
  `pnpm dev:web`).
- A runtime capability check (`lib/tauri.ts`) detects whether the Tauri
  IPC bridge is present. In the browser, the UI runs in preview mode:
  full shell, navigation, and design system; every mail feature shows
  its honest empty/unavailable state, and a status line labels the
  session as a browser preview.
- The frontend/backend boundary is unchanged: no protocol, SQL, MIME,
  or credential code enters the frontend for the web build. All Tauri
  calls go through one safe wrapper that degrades gracefully.
- A future real web client would need its own backend service and a
  dedicated ADR; nothing in this decision precommits to that.

## Consequences

- One codebase, two targets; CI's existing frontend build already
  produces the web artifact.
- UI code must never assume the IPC bridge exists — enforced by routing
  every invoke through the safe wrapper.
- The preview honestly demonstrates the UI without faking mail
  functionality, in line with the "no placeholder implementations that
  look finished" rule.
