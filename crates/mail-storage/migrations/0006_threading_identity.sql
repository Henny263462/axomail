-- Threading identity columns (RFC 5322 §3.6.4) used to group messages into
-- conversations: the message's own Message-ID, its first In-Reply-To parent,
-- and the ordered References chain.
--
-- `references_json` is a JSON array of message-ids (angle brackets stripped),
-- following the `address_json` precedent for JSON-valued columns. It also
-- doubles as the "threading extracted" marker: NULL means the columns have
-- not been populated yet (envelope-only IMAP ingest), while a JSON array
-- (including "[]") marks a row whose full headers have been scanned. The
-- lazy backfill on first open keys off this NULL sentinel.
--
-- All columns are nullable, so this is an in-place ALTER on the STRICT
-- `messages` table (SQLite adds TEXT columns to STRICT tables without a
-- rewrite).
ALTER TABLE messages ADD COLUMN message_id TEXT;
ALTER TABLE messages ADD COLUMN in_reply_to TEXT;
ALTER TABLE messages ADD COLUMN references_json TEXT;

-- Look up a message by its Message-ID (thread assembly, reply matching).
-- Partial: rows without a Message-ID are the uninteresting case and never
-- need indexing, keeping the index small.
CREATE INDEX idx_messages_message_id
    ON messages (message_id)
    WHERE message_id IS NOT NULL;
