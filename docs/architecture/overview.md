# Architecture overview

axomail separates the application into three layers with hard boundaries.

```
┌──────────────────────────────────────────────┐
│ Vue 3 frontend (apps/desktop/src)            │
│ rendering · navigation · transient UI state  │
└───────────────▲──────────────────────────────┘
                │ typed Tauri commands + events
┌───────────────┴──────────────────────────────┐
│ Tauri shell (apps/desktop/src-tauri)         │
│ command surface · events · capabilities      │
└───────────────▲──────────────────────────────┘
                │ plain Rust APIs
┌───────────────┴──────────────────────────────┐
│ Mail core (crates/)                          │
│                                              │
│  mail-core      models · IDs · traits ·      │
│                 MailError                    │
│  mail-imap ─┐                                │
│  mail-pop3 ─┼── protocol adapters behind     │
│  mail-smtp ─┘   mail-core traits             │
│  mail-sync      job engine (cancel/resume)   │
│  mail-storage   SQLite + attachment store    │
│  mail-parser    MIME → internal model        │
│  mail-security  sanitizing · keychain ·      │
│                 redaction                    │
│  mail-test-support  fixtures + test servers  │
└──────────────────────────────────────────────┘
```

Rules that keep the boundaries hard:

- The frontend never connects to mail servers, stores credentials, runs
  SQL, parses MIME, or renders unsanitized mail HTML.
- Only protocol adapter crates may contain protocol-specific types.
- `mail-sync` and `mail-storage` depend on `mail-core` abstractions only.
- Credentials exist solely in the OS keychain; SQLite holds non-secret
  references (see ADR-0001).
- Untrusted mail content is sanitized in Rust before it reaches the
  WebView; the rendering isolation strategy gets its own ADR before the
  message view is implemented.

See [ADR-0001](../adr/0001-protocol-and-storage-architecture.md) for the
protocol and storage decisions and
[plan-phase-1.md](plan-phase-1.md) for the phased delivery plan.
