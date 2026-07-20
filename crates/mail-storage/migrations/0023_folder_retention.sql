-- Per-mailbox message-retention policy (track 5.5, UX polish).
--
-- A mailbox's retention policy bounds how much mail is *kept* in the folder.
-- The maintenance pass (`apply_folder_retention`) applies it through one of two
-- deliberately safe dispositions — it NEVER hard-deletes server mail:
--
--   * `retention_action = 'trash'`      — over-retention messages are moved to
--     the account's Trash folder through the existing guarded delete-to-Trash
--     path (an optimistic local move plus a `pending_ops` server op that
--     replays a move to Trash). This is exactly what a manual delete does; the
--     mail is never expunged from the server, only relocated. Applying it to
--     the Trash folder itself is a no-op (source == target).
--   * `retention_action = 'purge_body'` — over-retention messages keep their
--     envelope row but drop their locally-cached raw body (the `.eml` blob is
--     removed and `has_raw` cleared, exactly like the offline-cache cleanup).
--     Nothing leaves the server and the body is re-fetched on demand.
--
-- `retention_mode` decides which messages are "over-retention":
--   * 'keep_all'  — retention is off; the maintenance pass is a no-op.
--   * 'by_age'    — messages older than `retention_days` days.
--   * 'by_count'  — every message beyond the newest `retention_max_count`.
--
-- Starred (`\Flagged`) messages are always exempt, so retention can never
-- silently discard a message the user marked important.

ALTER TABLE mailboxes
    ADD COLUMN retention_mode TEXT NOT NULL DEFAULT 'keep_all'
        CHECK (retention_mode IN ('keep_all', 'by_age', 'by_count'));

ALTER TABLE mailboxes
    ADD COLUMN retention_days INTEGER NOT NULL DEFAULT 30
        CHECK (retention_days > 0);

ALTER TABLE mailboxes
    ADD COLUMN retention_max_count INTEGER NOT NULL DEFAULT 1000
        CHECK (retention_max_count > 0);

ALTER TABLE mailboxes
    ADD COLUMN retention_action TEXT NOT NULL DEFAULT 'trash'
        CHECK (retention_action IN ('trash', 'purge_body'));
