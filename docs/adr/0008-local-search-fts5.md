# 0008. Local search via SQLite FTS5

Date: 2026-07-18
Status: accepted

## Context

Phase 4 needs local full-text search over headers and message text.
The default assumption (project brief §10) is SQLite FTS5; a dedicated
engine (e.g. tantivy) would add a large dependency and a second on-disk
index format.

## Decision

Use **SQLite FTS5** with an external-content table backed by the
message store:

- `messages_fts` indexes subject, sender/recipient names and
  addresses, and the extracted plain-text body.
- External-content mode keeps text out of the index twice; triggers on
  the content tables keep it in sync inside the same transaction.
- `unicode61` tokenizer with `remove_diacritics 2`; prefix indexes for
  as-you-type search (`prefix='2 3'`).
- Query surface lives in `axomail-storage` behind a typed API; the
  search index state table (brief §10) tracks indexing progress so a
  crash resumes cleanly.

Revisit (new ADR) only if measured search latency on ≥50k messages
misses the performance goals — tantivy is the documented fallback.

## Consequences

- No new dependency: sqlx's bundled SQLite ships FTS5.
- Ranking is BM25 (FTS5 built-in) — adequate for mailbox search;
  no fuzzy matching, which is accepted for the MVP.
- Body text must be extracted at store time (parser already produces
  it for previews), keeping search updates incremental.
