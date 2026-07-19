-- OAuth2 / XOAUTH2 authentication for accounts (E3).
--
-- `auth_mechanism` selects how an account authenticates. Existing rows and
-- new password accounts keep the default 'password'. The `oauth_*` columns
-- hold only NON-secret configuration (client id, endpoints, scopes); the
-- access and refresh tokens live exclusively in the OS keychain and never
-- touch this database (ADR-0001).
--
-- `oauth_scopes` stores the requested scopes as a single space-delimited
-- string (the OAuth wire form; individual scopes never contain spaces).

ALTER TABLE accounts ADD COLUMN auth_mechanism TEXT NOT NULL DEFAULT 'password'
    CHECK (auth_mechanism IN ('password', 'oauth2'));
ALTER TABLE accounts ADD COLUMN oauth_client_id TEXT;
ALTER TABLE accounts ADD COLUMN oauth_auth_url  TEXT;
ALTER TABLE accounts ADD COLUMN oauth_token_url TEXT;
ALTER TABLE accounts ADD COLUMN oauth_scopes    TEXT;
