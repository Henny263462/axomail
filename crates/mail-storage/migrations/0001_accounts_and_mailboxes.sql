-- Accounts and their per-account mailbox tree.
--
-- Only non-secret credential references are stored (`*_credential_ref`);
-- the referenced secrets live exclusively in the OS keychain and never
-- touch this database (ADR-0001).

CREATE TABLE accounts (
    id                      INTEGER PRIMARY KEY,
    display_name            TEXT    NOT NULL,
    email                   TEXT    NOT NULL,
    incoming_protocol       TEXT    NOT NULL CHECK (incoming_protocol IN ('imap', 'pop3')),
    incoming_host           TEXT    NOT NULL,
    incoming_port           INTEGER NOT NULL CHECK (incoming_port BETWEEN 1 AND 65535),
    incoming_security       TEXT    NOT NULL CHECK (incoming_security IN ('tls', 'starttls')),
    incoming_username       TEXT    NOT NULL,
    incoming_credential_ref TEXT    NOT NULL,
    outgoing_host           TEXT    NOT NULL,
    outgoing_port           INTEGER NOT NULL CHECK (outgoing_port BETWEEN 1 AND 65535),
    outgoing_security       TEXT    NOT NULL CHECK (outgoing_security IN ('tls', 'starttls')),
    outgoing_username       TEXT    NOT NULL,
    outgoing_credential_ref TEXT    NOT NULL,
    created_at_unix         INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE TABLE mailboxes (
    id           INTEGER PRIMARY KEY,
    account_id   INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    role         TEXT    NOT NULL CHECK (
        role IN ('inbox', 'drafts', 'sent', 'trash', 'archive', 'junk', 'custom')
    ),
    -- Remote (IMAP) folder state; NULL for POP3 or not-yet-synced folders.
    uid_validity INTEGER,
    uid_next     INTEGER,
    UNIQUE (account_id, name)
) STRICT;

CREATE INDEX idx_mailboxes_account ON mailboxes (account_id);

-- IMAP synchronization bookkeeping, one row per mailbox. Kept separate
-- from `mailboxes` so sync progress can be reset without touching the
-- folder identity.
CREATE TABLE imap_sync_state (
    mailbox_id     INTEGER PRIMARY KEY REFERENCES mailboxes (id) ON DELETE CASCADE,
    uid_validity   INTEGER,
    last_seen_uid  INTEGER,
    highest_modseq INTEGER,
    last_sync_unix INTEGER
) STRICT;
