-- Track 5.1 (S3 OpenPGP, ADR-0015). Non-secret OpenPGP key metadata.
--
-- Secret key material for our OWN keys never enters this database: it lives in
-- the OS keychain (ADR-0001), referenced by fingerprint. Only public keys and
-- their metadata are stored here — the public key is not a secret and is needed
-- for verification and encryption lookups. Recipient (contact) public keys used
-- to encrypt outgoing mail are stored the same way.

-- Our own keypairs. The armored public half is stored; the armored secret half
-- is in the keychain under `pgp/key/<fingerprint>`.
CREATE TABLE pgp_own_keys (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Full key fingerprint, lowercase hex, no separators. Unique identity.
    fingerprint     TEXT NOT NULL UNIQUE,
    -- Long key id, lowercase hex (the fingerprint's low 64 bits for v4 keys).
    key_id          TEXT NOT NULL,
    -- Primary user id string (conventionally `Name <email>`).
    user_id         TEXT NOT NULL,
    -- Public-key algorithm of the primary key (e.g. `EdDSALegacy`).
    algorithm       TEXT NOT NULL,
    -- ASCII-armored public key (non-secret).
    public_armored  TEXT NOT NULL,
    -- Whether the keychain-held secret key is passphrase-protected.
    has_passphrase  INTEGER NOT NULL DEFAULT 0 CHECK (has_passphrase IN (0, 1)),
    -- Whether this is the default key used to sign outgoing mail.
    is_default      INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    -- Key creation time (from the key material), Unix seconds.
    created_at_unix INTEGER NOT NULL,
    -- When the key was generated/imported locally, Unix seconds.
    added_at_unix   INTEGER NOT NULL
) STRICT;

-- Recipient / contact public keys, keyed by the email they encrypt to.
CREATE TABLE pgp_recipient_keys (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Lowercased recipient email this key is used to encrypt to.
    email           TEXT NOT NULL,
    fingerprint     TEXT NOT NULL,
    key_id          TEXT NOT NULL,
    user_id         TEXT NOT NULL,
    algorithm       TEXT NOT NULL,
    public_armored  TEXT NOT NULL,
    created_at_unix INTEGER NOT NULL,
    added_at_unix   INTEGER NOT NULL
) STRICT;

-- One key per (email, fingerprint); re-importing the same key updates it.
CREATE UNIQUE INDEX idx_pgp_recipient_email_fingerprint
    ON pgp_recipient_keys (email, fingerprint);
-- Fast "do we have a key for this recipient?" lookups when enabling encryption.
CREATE INDEX idx_pgp_recipient_email ON pgp_recipient_keys (email);
