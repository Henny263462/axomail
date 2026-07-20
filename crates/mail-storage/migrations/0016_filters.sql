-- Wave 4 track 4.2: message filters / rules engine.
--
-- A filter is an ordered rule owned by one account, or by *all* accounts when
-- `account_id` is NULL (a global rule). Each filter has a set of conditions
-- combined with `match_type` ('all' = AND, 'any' = OR) and an ordered list of
-- actions. `sort_order` fixes evaluation order within an account's rule list;
-- `stop_on_match` (and the separate `stop` action) halt evaluation of later
-- filters for a message once this one matches.
--
-- Conditions and actions are child rows (cascade on filter delete). Their
-- `field`/`op`/`kind` domains mirror the Rust enums exactly via CHECK
-- constraints; `position` preserves the row order the user built. All three
-- tables are STRICT (ADR-0001).

CREATE TABLE filters (
    id            INTEGER PRIMARY KEY,
    -- Owning account, or NULL for a rule that runs on every account.
    account_id    INTEGER REFERENCES accounts (id) ON DELETE CASCADE,
    -- User-visible rule name.
    name          TEXT    NOT NULL,
    -- Whether the rule participates in evaluation at all.
    enabled       INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    -- 'all' = every condition must match (AND); 'any' = at least one (OR).
    match_type    TEXT    NOT NULL DEFAULT 'all' CHECK (match_type IN ('all', 'any')),
    -- Evaluation order within a rule list (lower runs first).
    sort_order    INTEGER NOT NULL DEFAULT 0,
    -- When 1, a match stops evaluation of later filters for the message.
    stop_on_match INTEGER NOT NULL DEFAULT 0 CHECK (stop_on_match IN (0, 1)),
    created_at_unix INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- Rule-list order: account-scoped rules first, then the sort key, then id.
-- A partial index also serves the global (account_id IS NULL) lookups.
CREATE INDEX idx_filters_account ON filters (account_id, sort_order, id);

CREATE TABLE filter_conditions (
    id        INTEGER PRIMARY KEY,
    filter_id INTEGER NOT NULL REFERENCES filters (id) ON DELETE CASCADE,
    -- Which part of the message the condition tests.
    field     TEXT    NOT NULL CHECK (field IN (
                  'from', 'to', 'cc', 'subject', 'body',
                  'has_attachment', 'size', 'date', 'tag')),
    -- How the field is compared to `value`.
    op        TEXT    NOT NULL CHECK (op IN (
                  'contains', 'not_contains', 'is', 'is_not',
                  'starts_with', 'ends_with', 'matches_regex',
                  'greater_than', 'less_than')),
    -- The comparison operand (a substring, exact value, regex, number, or
    -- date/boolean rendered as text). Interpreted per `field`/`op` in Rust.
    value     TEXT    NOT NULL DEFAULT '',
    -- Order the user arranged the condition rows in.
    position  INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE INDEX idx_filter_conditions ON filter_conditions (filter_id, position, id);

CREATE TABLE filter_actions (
    id        INTEGER PRIMARY KEY,
    filter_id INTEGER NOT NULL REFERENCES filters (id) ON DELETE CASCADE,
    -- What the action does when its filter matches.
    kind      TEXT    NOT NULL CHECK (kind IN (
                  'move', 'copy', 'tag', 'star', 'mark_read', 'mark_unread',
                  'mark_junk', 'delete_to_trash', 'forward', 'set_priority',
                  'stop')),
    -- Action operand: a target mailbox id (move/copy), a tag name (tag),
    -- a forward address (forward), a priority level (set_priority), or empty.
    param     TEXT    NOT NULL DEFAULT '',
    -- Order the actions run in.
    position  INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE INDEX idx_filter_actions ON filter_actions (filter_id, position, id);

-- Per-account run-on-arrival bookkeeping (not one of the three rule tables):
-- the `enabled` toggle and the message-id high-water mark that keeps a new-mail
-- filter pass from reprocessing messages it has already considered. Message ids
-- are monotonic, so "inbox messages with id greater than last_message_id" is
-- exactly the set that arrived since the previous pass. A row is created
-- (baselined to the current inbox frontier) the first time arrival filtering
-- runs or is enabled, so turning filtering on never retroactively sweeps the
-- existing inbox.
CREATE TABLE filter_arrival_state (
    account_id       INTEGER PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    enabled          INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    last_message_id  INTEGER NOT NULL DEFAULT 0
) STRICT;
