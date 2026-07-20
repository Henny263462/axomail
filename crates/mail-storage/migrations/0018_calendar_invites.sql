-- Wave 4 track 4.4: iCalendar (iMIP) invitation RSVP state.
--
-- The mail client does NOT store a calendar; this table holds only the RSVP
-- state for a `text/calendar` REQUEST that arrived as a message, so a reopened
-- invitation shows the response the user already gave and a later CANCEL for
-- the same event can be reflected. The event's presentation (organizer, title,
-- time, agenda) is re-derived from the message on open; the columns here are a
-- lightweight snapshot plus the response.
--
-- One row per message: the `message_id` is unique, and the foreign key is an
-- ownership edge (deleting the message drops its invitation state).

CREATE TABLE calendar_invites (
    id           INTEGER PRIMARY KEY,
    -- The message carrying the invitation. Unique: one invitation per message.
    message_id   INTEGER NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    -- The event's globally-unique iCalendar UID (RFC 5545). Not unique here: a
    -- REQUEST and a later CANCEL are two messages sharing one UID.
    uid          TEXT    NOT NULL,
    -- Snapshot of the event title, organizer email, start/end (Unix seconds),
    -- and location for display without re-parsing. NULL when the invitation
    -- omitted them.
    summary      TEXT,
    organizer    TEXT,
    starts_at    INTEGER,
    ends_at      INTEGER,
    location     TEXT,
    -- The user's response. 'none' until they accept/decline.
    my_status    TEXT    NOT NULL DEFAULT 'none'
                 CHECK (my_status IN ('none', 'accepted', 'tentative', 'declined')),
    -- The REQUEST's SEQUENCE at the time of response, so a superseding update
    -- can be recognized.
    sequence     INTEGER NOT NULL DEFAULT 0,
    -- Set when a CANCEL for this UID has been seen: the event is off.
    cancelled    INTEGER NOT NULL DEFAULT 0,
    created_at_unix INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (message_id)
) STRICT;

-- CANCEL handling marks every stored invitation for a UID cancelled.
CREATE INDEX idx_calendar_invites_uid ON calendar_invites (uid);
