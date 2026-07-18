-- Local composition state: drafts and the outgoing send queue.
--
-- Recipient lists are stored as JSON arrays of address objects so the
-- composer can round-trip them without a join; the authoritative parsed
-- form is rebuilt at send time.

CREATE TABLE drafts (
    id                     INTEGER PRIMARY KEY,
    account_id             INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    in_reply_to_message_id INTEGER REFERENCES messages (id) ON DELETE SET NULL,
    subject                TEXT    NOT NULL DEFAULT '',
    body_text              TEXT    NOT NULL DEFAULT '',
    to_json                TEXT    NOT NULL DEFAULT '[]',
    cc_json                TEXT    NOT NULL DEFAULT '[]',
    bcc_json               TEXT    NOT NULL DEFAULT '[]',
    updated_at_unix        INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX idx_drafts_account ON drafts (account_id);

CREATE TABLE outbox (
    id                INTEGER PRIMARY KEY,
    account_id        INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    draft_id          INTEGER REFERENCES drafts (id) ON DELETE SET NULL,
    subject           TEXT    NOT NULL DEFAULT '',
    body_text         TEXT    NOT NULL DEFAULT '',
    to_json           TEXT    NOT NULL DEFAULT '[]',
    cc_json           TEXT    NOT NULL DEFAULT '[]',
    bcc_json          TEXT    NOT NULL DEFAULT '[]',
    state             TEXT    NOT NULL DEFAULT 'queued' CHECK (
        state IN ('queued', 'sending', 'sent', 'failed')
    ),
    attempts          INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    -- Redacted last failure reason for diagnostics; never carries secrets.
    last_error        TEXT,
    next_attempt_unix INTEGER,
    created_at_unix   INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at_unix   INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- Drives the outbox worker: pick queued/failed rows due for a retry.
CREATE INDEX idx_outbox_state ON outbox (state, next_attempt_unix);

-- POP3 UIDL de-duplication: a UIDL is unique within an account.
CREATE TABLE pop3_uidl (
    id              INTEGER PRIMARY KEY,
    account_id      INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    uidl            TEXT    NOT NULL,
    message_id      INTEGER REFERENCES messages (id) ON DELETE SET NULL,
    first_seen_unix INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (account_id, uidl)
) STRICT;
