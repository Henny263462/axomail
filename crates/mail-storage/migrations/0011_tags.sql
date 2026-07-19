-- Wave 2 track 2.4: tags / IMAP keywords.
--
-- A tag is the local, user-facing projection of an IMAP keyword (RFC 3501
-- custom flag). Each account owns its own tag set, so the same keyword synced
-- into two accounts is two independent tag rows. `keyword` stores the canonical
-- IMAP keyword atom; `display_name` and `color` are the presentation the user
-- edits. The keyword is the identity: it is unique per account and compared
-- case-insensitively (the `NOCASE` collation), matching the case-insensitive
-- identity of `axomail_core::Keyword` ($Junk and $junk are one keyword).
--
-- `message_tags` is the many-to-many join between messages and tags. Both edges
-- are ownership edges (a tag or message going away drops the association), so
-- both foreign keys cascade on delete.

CREATE TABLE tags (
    id              INTEGER PRIMARY KEY,
    account_id      INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- Canonical IMAP keyword atom; case-preserving on the wire but unique and
    -- compared case-insensitively within an account.
    keyword         TEXT    NOT NULL COLLATE NOCASE,
    -- Human-readable label shown on chips and in the tag manager.
    display_name    TEXT    NOT NULL,
    -- Presentation color as `#rrggbb`, or NULL for "no color" (auto-created
    -- inbound tags start colorless).
    color           TEXT,
    -- Display ordering within an account's tag list.
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at_unix INTEGER NOT NULL DEFAULT (unixepoch()),
    -- One tag per keyword per account (case-insensitive via the column
    -- collation), so keyword sync can upsert by (account_id, keyword).
    UNIQUE (account_id, keyword)
) STRICT;

CREATE INDEX idx_tags_account ON tags (account_id, sort_order, id);

-- Message ⇔ tag association. The primary key doubles as the tags-for-message
-- lookup index (WHERE message_id = ?); a separate index serves the reverse
-- messages-for-tag lookup and the in-use count.
CREATE TABLE message_tags (
    message_id INTEGER NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    tag_id     INTEGER NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    PRIMARY KEY (message_id, tag_id)
) STRICT;

CREATE INDEX idx_message_tags_tag ON message_tags (tag_id, message_id);
