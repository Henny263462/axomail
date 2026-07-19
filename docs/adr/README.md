# Architecture Decision Records

Numbered, immutable records of significant architecture decisions. A
superseded ADR is not edited; a new ADR replaces it and links back.

| ADR | Title | Status |
| --- | ----- | ------ |
| [0001](0001-protocol-and-storage-architecture.md) | Protocol and storage architecture | accepted |
| [0002](0002-core-dependency-selection.md) | Core dependency selection | accepted |
| [0003](0003-error-strategy.md) | Error strategy | accepted |
| [0004](0004-async-traits-without-async-trait.md) | Native async traits, enum dispatch, no `async-trait` | accepted |
| [0005](0005-web-preview-build.md) | Browser-served web preview of the UI | accepted |
| [0006](0006-protocol-stack-selection.md) | Protocol and content library selection | accepted |
| [0007](0007-html-rendering-isolation.md) | HTML mail rendering isolation | accepted |
| [0008](0008-local-search-fts5.md) | Local search via SQLite FTS5 | accepted |
| [0009](0009-oauth2-http-and-opener.md) | OAuth2 HTTP client and system-browser opener | accepted |
| [0010](0010-attachment-fs-dialog.md) | Attachment file dialogs + Rust-side disk IO | accepted |
| [0011](0011-rich-compose-text-durable.md) | Rich compose: session-scoped HTML/attachments, text-only durable store | accepted |

## Format

```markdown
# NNNN. Title

Date: YYYY-MM-DD
Status: proposed | accepted | superseded by NNNN

## Context
## Decision
## Consequences
```
