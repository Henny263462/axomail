-- Full-text search over message headers and body text (ADR-0008).
--
-- The `messages` table cannot back an FTS5 external-content index
-- directly: the searchable text is spread across `messages` (subject),
-- `message_recipients` (sender/recipient names and addresses), and the
-- extracted plain-text body, which is not stored on `messages` at all.
-- We therefore denormalize the searchable text into a plain content
-- table, `message_search_content`, and back the FTS5 index with it in
-- external-content mode. This keeps the tokenized text out of the store
-- twice and lets triggers on the content table maintain the index inside
-- the same transaction as the write that produced it.

-- One row of searchable text per message. `message_id` aliases the rowid
-- so it lines up with `messages_fts` (`content_rowid`). Deleting a
-- message cascades here, and the delete trigger below then removes the
-- message from the index — so message removal needs no explicit FTS call.
CREATE TABLE message_search_content (
    message_id     INTEGER PRIMARY KEY REFERENCES messages (id) ON DELETE CASCADE,
    -- Decoded subject.
    subject        TEXT NOT NULL DEFAULT '',
    -- Denormalized sender/recipient display names and addresses.
    correspondents TEXT NOT NULL DEFAULT '',
    -- Extracted plain-text body, filled in when a message is opened.
    body           TEXT NOT NULL DEFAULT ''
) STRICT;

-- External-content FTS5 index. `unicode61 remove_diacritics 2` folds
-- accents so "café" matches "cafe"; `prefix='2 3'` builds prefix indexes
-- for responsive as-you-type (2- and 3-character) prefix search.
CREATE VIRTUAL TABLE messages_fts USING fts5 (
    subject,
    correspondents,
    body,
    content='message_search_content',
    content_rowid='message_id',
    tokenize='unicode61 remove_diacritics 2',
    prefix='2 3'
);

-- Keep the index synchronized with the content table inside the writing
-- transaction. The 'delete' command rows feed FTS5 the old column values
-- it needs to locate and remove an entry in external-content mode.
CREATE TRIGGER message_search_content_ai AFTER INSERT ON message_search_content BEGIN
    INSERT INTO messages_fts (rowid, subject, correspondents, body)
    VALUES (new.message_id, new.subject, new.correspondents, new.body);
END;

CREATE TRIGGER message_search_content_ad AFTER DELETE ON message_search_content BEGIN
    INSERT INTO messages_fts (messages_fts, rowid, subject, correspondents, body)
    VALUES ('delete', old.message_id, old.subject, old.correspondents, old.body);
END;

CREATE TRIGGER message_search_content_au AFTER UPDATE ON message_search_content BEGIN
    INSERT INTO messages_fts (messages_fts, rowid, subject, correspondents, body)
    VALUES ('delete', old.message_id, old.subject, old.correspondents, old.body);
    INSERT INTO messages_fts (rowid, subject, correspondents, body)
    VALUES (new.message_id, new.subject, new.correspondents, new.body);
END;
