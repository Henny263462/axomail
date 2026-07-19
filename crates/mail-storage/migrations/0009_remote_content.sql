-- Per-sender "always show remote content" allow list (Track 1.6).
--
-- A message's remote images stay blocked until the user either shows them once
-- for that message or adds the sender here to allow them for every future
-- message from that address. The sender email is stored already normalized
-- (trimmed and lowercased) so a case-insensitive From matches on a primary-key
-- lookup with no scan. Rows are independent of accounts and messages on
-- purpose: the decision is about a correspondent, and it must outlive any
-- particular message or a re-added account.
CREATE TABLE remote_content_senders (
    sender          TEXT    PRIMARY KEY NOT NULL,
    created_at_unix INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;
