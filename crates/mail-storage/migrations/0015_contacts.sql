-- Wave 4 track 4.1: contacts / address book.
--
-- The address book is global (not per-account): a person is the same person
-- regardless of which account you mail them from, matching Thunderbird's shared
-- Personal Address Book / Collected Addresses model. Auto-collection on send
-- (a "collected addresses" notion, toggleable in settings) upserts recipient
-- addresses here so they become available to recipient autocomplete.
--
-- A contact carries one denormalized `primary_email` for cheap list rendering;
-- the authoritative, possibly-multiple addresses live in `contact_emails`. The
-- repository keeps `primary_email` in sync on every write (it mirrors the
-- `is_primary` row, falling back to the first address), so callers never edit
-- the two out of step.
--
-- `contact_emails.email` is indexed and compared case-insensitively (the
-- `NOCASE` collation) so both upsert-by-email (auto-collection dedup) and
-- prefix autocomplete lookups are index-served and case-agnostic. Optional
-- groups (`contact_groups` + `contact_group_members`) model mailing lists: a
-- group name typed into a recipient field expands to its members' addresses.

CREATE TABLE contacts (
    id              INTEGER PRIMARY KEY,
    -- Human-readable name shown in the list and inserted (with the address) by
    -- autocomplete. Never blank: the repository derives one from the address
    -- when a contact is auto-collected without a name.
    display_name    TEXT    NOT NULL,
    -- Denormalized primary address (mirrors the `is_primary` `contact_emails`
    -- row), kept in sync by the repository. NULL only for a name-only contact
    -- with no addresses.
    primary_email   TEXT    COLLATE NOCASE,
    -- Free-form note; empty string when unset (never NULL, so reads are total).
    note            TEXT    NOT NULL DEFAULT '',
    created_at_unix INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at_unix INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- Name index for the alphabetical address-book listing and name-prefix search.
CREATE INDEX idx_contacts_name ON contacts (display_name, id);

CREATE TABLE contact_emails (
    id          INTEGER PRIMARY KEY,
    -- Owning contact; deleting the contact drops all its addresses (ownership
    -- edge).
    contact_id  INTEGER NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
    -- The address. Case-insensitive identity so upsert-by-email and search are
    -- case-agnostic (Ada@x.org and ada@x.org are one address).
    email       TEXT    NOT NULL COLLATE NOCASE,
    -- Presentation label ("home", "work", ...); empty string when unlabeled.
    label       TEXT    NOT NULL DEFAULT '',
    -- Exactly one address per contact is the primary (repository invariant).
    is_primary  INTEGER NOT NULL DEFAULT 0,
    -- A contact never lists the same address twice (case-insensitive via the
    -- column collation), so import/collect can upsert by (contact_id, email).
    UNIQUE (contact_id, email)
) STRICT;

-- Email index for prefix autocomplete and O(1) upsert-by-email dedup. Leading
-- column is `email` so `WHERE email LIKE 'prefix%'` and `WHERE email = ?` both
-- use it.
CREATE INDEX idx_contact_emails_email ON contact_emails (email, contact_id);
-- Per-contact address read (primary first), for the list and detail views.
CREATE INDEX idx_contact_emails_contact
    ON contact_emails (contact_id, is_primary DESC, id);

CREATE TABLE contact_groups (
    id              INTEGER PRIMARY KEY,
    -- Group / mailing-list name, unique case-insensitively so a name typed into
    -- a recipient field resolves to exactly one group.
    name            TEXT    NOT NULL COLLATE NOCASE,
    created_at_unix INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (name)
) STRICT;

-- Group membership (many-to-many). Both edges are ownership edges: deleting a
-- group or a contact drops the association but never the other side.
CREATE TABLE contact_group_members (
    group_id   INTEGER NOT NULL REFERENCES contact_groups (id) ON DELETE CASCADE,
    contact_id INTEGER NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, contact_id)
) STRICT;

-- Reverse lookup: which groups a contact belongs to (the PK serves the forward
-- members-of-group direction).
CREATE INDEX idx_contact_group_members_contact
    ON contact_group_members (contact_id, group_id);
