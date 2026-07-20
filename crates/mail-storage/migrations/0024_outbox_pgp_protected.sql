-- Wave 5 adversarial fix: mark outbox rows that were submitted with OpenPGP
-- protection (sign and/or encrypt) so a failed protected send can never be
-- retried and re-transmitted in the clear.
--
-- PGP protection is resolved only on the immediate-send path (the durable row is
-- text-only and the signing passphrase is never persisted), so a background or
-- manual retry that rebuilt the row from its stored columns would send the
-- plaintext. Recording the intent lets `retry_outbox` refuse such a row: the
-- user must re-compose. The column is non-secret (a single flag), so it does not
-- breach the no-secrets-in-DB rule (ADR-0001).
--
-- `next_attempt_unix`/rows already existing default to 0 (not protected), which
-- is correct: any pre-existing row was a plaintext send.

ALTER TABLE outbox
    ADD COLUMN pgp_protected INTEGER NOT NULL DEFAULT 0
        CHECK (pgp_protected IN (0, 1));
