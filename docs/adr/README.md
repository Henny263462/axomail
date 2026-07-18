# Architecture Decision Records

Numbered, immutable records of significant architecture decisions. A
superseded ADR is not edited; a new ADR replaces it and links back.

| ADR | Title | Status |
| --- | ----- | ------ |
| [0001](0001-protocol-and-storage-architecture.md) | Protocol and storage architecture | accepted |
| [0002](0002-core-dependency-selection.md) | Core dependency selection | accepted |

Planned next (work package 1b): error strategy, workspace layout details,
native `async fn` in traits vs. `async-trait`.

## Format

```markdown
# NNNN. Title

Date: YYYY-MM-DD
Status: proposed | accepted | superseded by NNNN

## Context
## Decision
## Consequences
```
