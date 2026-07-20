-- Wave 5 track 5.2: saved searches (virtual folders).
--
-- A saved search is a named, reusable advanced query. It has three parts:
--   * query_json — the condition set: {"matchType": "all"|"any",
--                  "conditions": [{"field","op","value"}, …]}. The field/op
--                  domains mirror the Rust `SearchField` / `ConditionOp` enums
--                  (the latter reused verbatim from the Wave-4 filter model), so
--                  the two subsystems share one condition representation.
--   * scope     — the structural narrowing, serialized as
--                  {"kind":"all_folders"} | {"kind":"folder","mailboxId":N} |
--                  {"kind":"folder_recursive","mailboxId":N}. Kept as opaque
--                  JSON (not a foreign key) so a saved search survives the
--                  deletion of the folder it scoped to — it then simply resolves
--                  to an empty candidate set rather than erroring.
--   * sort      — the result ordering, a small closed enum (CHECK-mirrored).
--
-- Saved searches are global (they can span every account, like the full-text
-- search box), so there is no owning-account foreign key: deleting an account
-- leaves the saved search intact; a folder-scoped one whose folder vanished
-- runs empty. The table is STRICT (ADR-0001).

CREATE TABLE saved_searches (
    id              INTEGER PRIMARY KEY,
    -- User-visible name shown in the sidebar "Saved searches" group.
    name            TEXT    NOT NULL,
    -- The condition set (match type + conditions) as JSON; see above.
    query_json      TEXT    NOT NULL,
    -- The structural scope as JSON; see above.
    scope           TEXT    NOT NULL,
    -- Result ordering. Mirrors the Rust `SearchSort` enum exactly.
    sort            TEXT    NOT NULL DEFAULT 'date_desc'
                        CHECK (sort IN ('date_desc', 'date_asc',
                                        'subject_asc', 'subject_desc')),
    created_at_unix INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- Sidebar listing order: alphabetical, case-insensitive, id as a stable
-- tiebreaker for equal names.
CREATE INDEX idx_saved_searches_name ON saved_searches (name COLLATE NOCASE, id);
