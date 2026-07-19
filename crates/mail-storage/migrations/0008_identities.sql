-- Sender identities: one account can present several From identities, each
-- with its own display name, reply-to, signature, and composition
-- preferences. Exactly one identity per account is the default (enforced
-- jointly by the partial unique index below and the repository, which never
-- leaves an account without a default).
--
-- The `*_dest` columns are non-secret references to mailboxes an identity
-- files copies into (Sent/Drafts/Templates/Archive). They are nullable and
-- use ON DELETE SET NULL so removing a folder never deletes an identity; they
-- are wired into filing behaviour by a later wave and default to NULL here.

CREATE TABLE identities (
    id                  INTEGER PRIMARY KEY,
    account_id          INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    email               TEXT    NOT NULL,
    display_name        TEXT    NOT NULL DEFAULT '',
    reply_to            TEXT,
    signature_text      TEXT,
    signature_placement TEXT    NOT NULL DEFAULT 'below_quote' CHECK (
        signature_placement IN ('below_quote', 'above_quote')
    ),
    reply_position      TEXT    NOT NULL DEFAULT 'below' CHECK (
        reply_position IN ('above', 'below')
    ),
    forward_mode        TEXT    NOT NULL DEFAULT 'inline' CHECK (
        forward_mode IN ('inline', 'attachment')
    ),
    sent_dest           INTEGER REFERENCES mailboxes (id) ON DELETE SET NULL,
    drafts_dest         INTEGER REFERENCES mailboxes (id) ON DELETE SET NULL,
    templates_dest      INTEGER REFERENCES mailboxes (id) ON DELETE SET NULL,
    archive_dest        INTEGER REFERENCES mailboxes (id) ON DELETE SET NULL,
    archive_granularity TEXT    NOT NULL DEFAULT 'flat' CHECK (
        archive_granularity IN ('flat', 'yearly', 'monthly')
    ),
    is_default          INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    sort_position       INTEGER NOT NULL DEFAULT 0,
    created_at_unix     INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX idx_identities_account ON identities (account_id);

-- At most one default identity per account. The repository guarantees at
-- least one, so together they make the default exactly one per account.
CREATE UNIQUE INDEX idx_identities_one_default
    ON identities (account_id) WHERE is_default = 1;

-- Seed one default identity per existing account from its own address and
-- display name, so every account has a usable From identity immediately.
INSERT INTO identities (account_id, email, display_name, is_default, sort_position)
SELECT id, email, display_name, 1, 0 FROM accounts;

-- Scheduled ("send later") sends live in the outbox as queued rows whose
-- next_attempt_unix is in the future; the selection guard keeps the worker
-- from picking them up early. An identity reference records which From the
-- message will be sent under when it is finally submitted.
ALTER TABLE outbox
    ADD COLUMN identity_id INTEGER REFERENCES identities (id) ON DELETE SET NULL;
