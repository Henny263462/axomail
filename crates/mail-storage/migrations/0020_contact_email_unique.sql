-- Wave 4 review fix (track 4.1): make address auto-collection race-free.
--
-- Auto-collection on send did a check-then-insert (`SELECT ... ; INSERT ...`),
-- so two concurrent sends to the same new address could both pass the existence
-- check and each create a contact — a duplicate. The `contact_emails` table
-- only guaranteed uniqueness *within* a contact (`UNIQUE (contact_id, email)`),
-- not across the book.
--
-- Enforce that an address identifies at most one contact by adding a global
-- unique index on `contact_emails.email` (case-insensitive via the column's
-- NOCASE collation). The repository then upserts atomically and treats a unique
-- violation as "already collected", closing the race.
--
-- Defensive dedup first: should any cross-contact duplicate already exist (from
-- the race this fixes), keep the earliest row so the index can be created. This
-- only ever touches addresses duplicated across *different* contacts, which the
-- address-book model treats as the same person.
DELETE FROM contact_emails
WHERE id NOT IN (
    SELECT MIN(id) FROM contact_emails GROUP BY email
);

CREATE UNIQUE INDEX idx_contact_emails_email_unique ON contact_emails (email);
