-- Message envelopes, their recipients, and attachment metadata.
--
-- Raw message bytes and attachment payloads live outside the database
-- (attachment store on disk); only metadata and a `has_raw` marker are
-- kept here (ADR-0001).

CREATE TABLE messages (
    id                  INTEGER PRIMARY KEY,
    mailbox_id          INTEGER NOT NULL REFERENCES mailboxes (id) ON DELETE CASCADE,
    -- Remote identity. NULL columns mean "local-only" (e.g. a draft).
    remote_kind         TEXT    CHECK (remote_kind IN ('imap', 'pop3')),
    remote_uid          INTEGER,
    remote_uid_validity INTEGER,
    remote_uidl         TEXT,
    subject             TEXT,
    date_unix           INTEGER,
    flag_seen           INTEGER NOT NULL DEFAULT 0 CHECK (flag_seen IN (0, 1)),
    flag_answered       INTEGER NOT NULL DEFAULT 0 CHECK (flag_answered IN (0, 1)),
    flag_flagged        INTEGER NOT NULL DEFAULT 0 CHECK (flag_flagged IN (0, 1)),
    flag_draft          INTEGER NOT NULL DEFAULT 0 CHECK (flag_draft IN (0, 1)),
    flag_deleted        INTEGER NOT NULL DEFAULT 0 CHECK (flag_deleted IN (0, 1)),
    size_bytes          INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    preview             TEXT,
    has_raw             INTEGER NOT NULL DEFAULT 0 CHECK (has_raw IN (0, 1)),
    created_at_unix     INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- Primary list ordering: newest first within a mailbox, id as tiebreaker.
CREATE INDEX idx_messages_mailbox_date ON messages (mailbox_id, date_unix DESC, id DESC);

-- One IMAP UID identifies at most one message per mailbox generation.
CREATE UNIQUE INDEX idx_messages_remote_uid
    ON messages (mailbox_id, remote_uid_validity, remote_uid)
    WHERE remote_kind = 'imap';

CREATE TABLE message_recipients (
    id           INTEGER PRIMARY KEY,
    message_id   INTEGER NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    kind         TEXT    NOT NULL CHECK (kind IN ('from', 'to', 'cc', 'bcc')),
    display_name TEXT,
    email        TEXT    NOT NULL,
    position     INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE INDEX idx_recipients_message ON message_recipients (message_id, kind, position);

CREATE TABLE attachments (
    id              INTEGER PRIMARY KEY,
    message_id      INTEGER NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    filename        TEXT,
    mime_type       TEXT    NOT NULL,
    size_bytes      INTEGER NOT NULL CHECK (size_bytes >= 0),
    content_hash    TEXT    NOT NULL,
    stored_path     TEXT    NOT NULL,
    created_at_unix INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX idx_attachments_message ON attachments (message_id);
