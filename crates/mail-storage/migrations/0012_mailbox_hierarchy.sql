-- Wave 2 track 2.2 (Folders): give mailboxes a hierarchy plus the columns the
-- discovery reconcile and the folder tree UI need.
--
-- New columns on `mailboxes`:
--   * parent_id     — nullable self-reference to the containing folder; ON
--                     DELETE SET NULL so deleting a parent orphans (never
--                     cascade-deletes) its children, preserving their messages.
--   * remote_name   — the full server path (e.g. "INBOX/Work/2026"). This is
--                     the authoritative identity the reconcile upserts by; the
--                     display `name` stays the (unique) full label so the tree
--                     can derive a leaf from `remote_name` + `delimiter`.
--   * delimiter     — the server's hierarchy delimiter for this folder ("/",
--                     ".", …); NULL for flat/local folders (POP3, seeds).
--   * is_subscribed — whether the folder is subscribed; default 1 so every
--                     discovered/seeded folder shows until the user opts out.
--   * is_orphaned   — set when a previously-discovered folder vanished from the
--                     server but still holds local messages: it is FLAGGED, not
--                     silently dropped (data-loss priority). Cleared when the
--                     folder reappears.
--   * discovered_at — Unix seconds of the last successful discovery of this
--                     folder. NULL means "never seen on the server" (a local
--                     seed), which the reconcile uses to decide orphan
--                     candidacy: only previously-discovered folders can orphan.
--
-- SQLite allows ADD COLUMN on STRICT tables. A foreign-key column added this
-- way must default to NULL (it does). Existing rows are backfilled so
-- `remote_name` is never NULL for a real folder.

ALTER TABLE mailboxes ADD COLUMN parent_id INTEGER REFERENCES mailboxes (id) ON DELETE SET NULL;
ALTER TABLE mailboxes ADD COLUMN remote_name TEXT;
ALTER TABLE mailboxes ADD COLUMN delimiter TEXT;
ALTER TABLE mailboxes ADD COLUMN is_subscribed INTEGER NOT NULL DEFAULT 1 CHECK (is_subscribed IN (0, 1));
ALTER TABLE mailboxes ADD COLUMN is_orphaned INTEGER NOT NULL DEFAULT 0 CHECK (is_orphaned IN (0, 1));
ALTER TABLE mailboxes ADD COLUMN discovered_at INTEGER;

-- Backfill: existing folders adopt their display name as the full remote path.
UPDATE mailboxes SET remote_name = name WHERE remote_name IS NULL;

-- The reconcile upserts by (account, remote_name); make that lookup unique and
-- indexed. remote_name is non-NULL for every real folder after the backfill.
CREATE UNIQUE INDEX idx_mailboxes_remote ON mailboxes (account_id, remote_name);

-- Nested-tree queries walk children by parent.
CREATE INDEX idx_mailboxes_parent ON mailboxes (parent_id);

-- Per-folder unread counts: a partial index over unread rows turns the former
-- O(messages) page scan (commands/messages.rs::count_unread) into an indexed
-- COUNT. Total counts are already served by idx_messages_mailbox_date's
-- mailbox_id prefix.
CREATE INDEX idx_messages_unread ON messages (mailbox_id) WHERE flag_seen = 0;
