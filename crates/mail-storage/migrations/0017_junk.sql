-- Track 4.3 (A3 Junk): the Bayesian classifier's per-account token store and a
-- per-message junk score.
--
-- `junk_tokens` holds, per account, how many trained ham and spam messages each
-- token appeared in; `junk_corpus` holds the per-account message totals (the
-- Bayesian denominators). Both cascade on account deletion so a removed account
-- leaves no classifier residue. `messages.junk_score` records the last computed
-- spam probability in 0..1 (NULL until scored).

CREATE TABLE junk_tokens (
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    token      TEXT    NOT NULL,
    ham_count  INTEGER NOT NULL DEFAULT 0,
    spam_count INTEGER NOT NULL DEFAULT 0,
    -- The composite primary key is the required (account_id, token) index: it
    -- makes the per-token upsert and the scoring lookup a single index probe.
    PRIMARY KEY (account_id, token)
) STRICT;

CREATE TABLE junk_corpus (
    account_id    INTEGER PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    ham_messages  INTEGER NOT NULL DEFAULT 0,
    spam_messages INTEGER NOT NULL DEFAULT 0
) STRICT;

ALTER TABLE messages ADD COLUMN junk_score REAL;
