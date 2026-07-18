-- Background job tracking, search-index progress, and key/value settings.

CREATE TABLE sync_jobs (
    id               INTEGER PRIMARY KEY,
    kind             TEXT    NOT NULL CHECK (
        kind IN ('discover', 'mailbox_sync', 'fetch', 'send')
    ),
    state            TEXT    NOT NULL DEFAULT 'pending' CHECK (
        state IN ('pending', 'running', 'succeeded', 'failed', 'cancelled')
    ),
    account_id       INTEGER REFERENCES accounts (id) ON DELETE CASCADE,
    mailbox_id       INTEGER REFERENCES mailboxes (id) ON DELETE CASCADE,
    started_at_unix  INTEGER,
    finished_at_unix INTEGER,
    -- Redacted failure reason for diagnostics; never carries secrets.
    error            TEXT,
    created_at_unix  INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX idx_sync_jobs_state ON sync_jobs (state, kind);

-- Full-text index progress per mailbox so a crash resumes cleanly
-- (ADR-0008). The FTS5 virtual table itself is added in phase 4.
CREATE TABLE search_index_state (
    mailbox_id              INTEGER PRIMARY KEY REFERENCES mailboxes (id) ON DELETE CASCADE,
    last_indexed_message_id INTEGER,
    last_indexed_unix       INTEGER,
    pending                 INTEGER NOT NULL DEFAULT 0 CHECK (pending IN (0, 1))
) STRICT;

CREATE TABLE app_settings (
    key             TEXT PRIMARY KEY,
    value           TEXT NOT NULL,
    updated_at_unix INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;
