-- Wave 4 review fix (track 4.3): idempotent, reversible junk training.
--
-- Records the class each message was *last* trained as, so the mark-junk /
-- not-junk path can be idempotent and reversible. Without this ledger, repeated
-- mark-junk clicks (or a mark-junk followed by a not-junk correction) each
-- bumped the per-account corpus and token counts unconditionally, inflating the
-- totals and poisoning the classifier.
--
-- The repository consults this table before training: re-training a message in
-- the class it already holds is a no-op, and switching its class reverses the
-- prior class's counts (clamped at zero) before applying the new class's.
--
-- One row per (account, message). Both foreign keys are ownership edges: the
-- ledger row is meaningless once its account or message is gone.
CREATE TABLE junk_training (
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    message_id INTEGER NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    -- The class the message is currently trained as: 1 = spam, 0 = ham.
    is_spam    INTEGER NOT NULL CHECK (is_spam IN (0, 1)),
    PRIMARY KEY (account_id, message_id)
) STRICT;
