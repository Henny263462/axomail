-- Wave 2 Stage 0: the offline operation queue plus the `op_replay` job kind.
--
-- Two changes ship together:
--   1. Extend the `sync_jobs.kind` CHECK domain with 'op_replay' (the job the
--      write-back track enqueues to drain `pending_ops`). SQLite cannot ALTER
--      a CHECK constraint, so the table is rebuilt in place. No other table
--      references `sync_jobs`, and the rows are FK-valid, so the rebuild is
--      safe inside the migration transaction.
--   2. Create `pending_ops`: an ordered, per-account, message-scoped queue of
--      offline operations with retry state.
--
-- The account-scoped dedup fix is a *code* change (create_if_absent now keys on
-- (kind, account_id, mailbox_id)); the `account_id` column already exists from
-- migration 0004, so no schema change is needed for it.

-- 1. Rebuild sync_jobs with the extended kind domain.
CREATE TABLE sync_jobs_new (
    id               INTEGER PRIMARY KEY,
    kind             TEXT    NOT NULL CHECK (
        kind IN ('discover', 'mailbox_sync', 'fetch', 'send', 'op_replay')
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

INSERT INTO sync_jobs_new (
    id, kind, state, account_id, mailbox_id,
    started_at_unix, finished_at_unix, error, created_at_unix
)
SELECT
    id, kind, state, account_id, mailbox_id,
    started_at_unix, finished_at_unix, error, created_at_unix
FROM sync_jobs;

DROP TABLE sync_jobs;
ALTER TABLE sync_jobs_new RENAME TO sync_jobs;

CREATE INDEX idx_sync_jobs_state ON sync_jobs (state, kind);

-- 2. The offline operation queue.
--
-- Ordering is `id` per account. A message's ops execute in id order: the
-- lowest-id un-completed op for a message is its "head", and only a *pending*
-- head is claimable, so an inflight or failed head blocks later ops for the
-- SAME message while ops for other messages keep flowing (a poison op wedges
-- one message chain, never the whole account). `payload_json` follows the
-- `address_json` precedent: opaque JSON text validated in Rust on enqueue.
CREATE TABLE pending_ops (
    id              INTEGER PRIMARY KEY,
    account_id      INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    message_id      INTEGER NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    kind            TEXT    NOT NULL CHECK (
        kind IN (
            'add_flags', 'remove_flags', 'add_keyword', 'remove_keyword',
            'move', 'delete'
        )
    ),
    payload_json    TEXT    NOT NULL,
    state           TEXT    NOT NULL DEFAULT 'pending' CHECK (
        state IN ('pending', 'inflight', 'failed')
    ),
    attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    -- Redacted last failure reason for diagnostics; never carries secrets.
    last_error      TEXT,
    created_at_unix INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at_unix INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- Drives claim_next_batch: scan an account's pending ops in id order.
CREATE INDEX idx_pending_ops_claim ON pending_ops (account_id, state, id);
-- Supports the per-message head check (earlier op for the same message?).
CREATE INDEX idx_pending_ops_message ON pending_ops (message_id, id);
