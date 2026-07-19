-- Per-mailbox offline policy and raw-store download bookkeeping (track 2.3).
--
-- `offline_mode` decides which of a mailbox's messages have their full raw
-- RFC 5322 body pre-downloaded during sync (cached under
-- `<raw_root>/raw/<id>.eml`, with `messages.has_raw` flipped to 1 and the body
-- indexed for search):
--   * 'none'   — download nothing eagerly; the reading pane still lazy-fetches
--                a body on first open (the pre-existing fallback).
--   * 'recent' — download bodies newer than `offline_recent_days` (default).
--   * 'all'    — download every message's body, oldest and newest alike.
--
-- `pending_full_download` is a transient "download now" marker: the folder
-- command sets it to 1 to force a one-off full download (as if 'all') on the
-- next sync passes, regardless of the standing policy; the sync worker clears
-- it once the backlog is drained.

ALTER TABLE mailboxes
    ADD COLUMN offline_mode TEXT NOT NULL DEFAULT 'recent'
        CHECK (offline_mode IN ('none', 'recent', 'all'));

ALTER TABLE mailboxes
    ADD COLUMN offline_recent_days INTEGER NOT NULL DEFAULT 30
        CHECK (offline_recent_days > 0);

ALTER TABLE mailboxes
    ADD COLUMN pending_full_download INTEGER NOT NULL DEFAULT 0
        CHECK (pending_full_download IN (0, 1));
