/**
 * Account setup, authentication, connection-test, and edit view models.
 *
 * Split out of `mail.ts` (barrel) to keep each file under the size gate. These
 * mirror the serialized shapes of the Rust account DTOs (serde `camelCase`);
 * passwords are sent once to the backend and never persisted in the frontend,
 * and OAuth tokens never cross this boundary at all.
 */

/** How an account authenticates to its servers. */
export type AuthMechanism = "password" | "oauth2";

/** Incoming-mail protocol chosen during account setup. */
export type IncomingProtocol = "imap" | "pop3";

/** Transport security for a mail server connection. */
export type TransportSecurity = "tls" | "starttls";

/** Incoming (IMAP/POP3) server configuration for a new account. */
export interface IncomingServerConfig {
  protocol: IncomingProtocol;
  host: string;
  port: number;
  security: TransportSecurity;
  username: string;
  /**
   * Plaintext password, sent once to the backend which stores it in the
   * OS keychain. Never logged, echoed, or persisted in the frontend.
   */
  password: string;
}

/** Outgoing (SMTP) server configuration for a new account. */
export interface OutgoingServerConfig {
  host: string;
  port: number;
  security: TransportSecurity;
  username: string;
  /**
   * Plaintext password, sent once to the backend which stores it in the
   * OS keychain. Never logged, echoed, or persisted in the frontend.
   */
  password: string;
}

/** A supported OAuth2 provider preset, or fully custom endpoints. */
export type OAuthProvider = "gmail" | "microsoft" | "custom";

/**
 * OAuth2 configuration from the setup form. Only non-secret values: a
 * provider preset (or custom), the user's own public client id, and optional
 * endpoint/scope overrides. Tokens are never carried here — they are obtained
 * by the separate browser sign-in and stored in the OS keychain.
 */
export interface OAuthInput {
  provider: OAuthProvider;
  clientId: string;
  /** Required for `custom`; overrides a preset when present. */
  authUrl?: string;
  tokenUrl?: string;
  scopes?: string[];
}

/** The authentication choice for a new account. */
export interface AuthInput {
  mechanism: AuthMechanism;
  /** Present only when `mechanism` is `oauth2`. */
  oauth?: OAuthInput;
}

/**
 * Payload for the `create_account` command. Mirrors the Rust
 * `CreateAccountInput` exactly (serde `camelCase`).
 */
export interface CreateAccountInput {
  displayName: string;
  email: string;
  auth: AuthInput;
  incoming: IncomingServerConfig;
  outgoing: OutgoingServerConfig;
}

/** Outcome of the OAuth2 browser sign-in (`oauth_authorize`). */
export type OAuthAuthorizeStatus = "authorized" | "denied" | "timedOut";

export interface OAuthAuthorizeResult {
  status: OAuthAuthorizeStatus;
}

/**
 * Result of a live connection test. Nothing is persisted by the command;
 * each side reports independently so the user sees which server failed.
 */
export interface ConnectionTestResult {
  incomingOk: boolean;
  incomingError: string | null;
  outgoingOk: boolean;
  outgoingError: string | null;
}

/**
 * Receiving-server view of an {@link AccountDetail}. `protocol` and
 * `username` are shown for context but are immutable through the edit path.
 */
export interface IncomingServerDetail {
  protocol: IncomingProtocol;
  host: string;
  port: number;
  security: TransportSecurity;
  username: string;
}

/** Sending-server view of an {@link AccountDetail}. */
export interface OutgoingServerDetail {
  host: string;
  port: number;
  security: TransportSecurity;
  username: string;
}

/**
 * Full, editable configuration of one account, mirroring the Rust
 * `AccountDetailDto`. Carries no password or credential reference: only the
 * display name and both servers' host/port/security are editable, with the
 * protocol, login names, and email shown for context.
 */
export interface AccountDetail {
  id: number;
  displayName: string;
  email: string;
  incoming: IncomingServerDetail;
  outgoing: OutgoingServerDetail;
}

/** One server's editable transport fields for {@link UpdateAccountInput}. */
export interface ServerEndpointInput {
  host: string;
  port: number;
  security: TransportSecurity;
}

/** Which discovery source produced an {@link AutoconfigResult}. */
export type AutoconfigSource = "autoconfig" | "ispdb" | "guess";

/** Suggested incoming (IMAP/POP3) server settings from auto-configuration. */
export interface AutoconfigIncoming {
  protocol: IncomingProtocol;
  host: string;
  port: number;
  security: TransportSecurity;
  username: string;
}

/** Suggested outgoing (SMTP) server settings from auto-configuration. */
export interface AutoconfigOutgoing {
  host: string;
  port: number;
  security: TransportSecurity;
  username: string;
}

/**
 * Outcome of the `discover_autoconfig` command. A struct (not a bare optional)
 * so `found: false` (searched, nothing found) is distinct from the `null` the
 * api layer returns in the browser preview. Never carries a secret: the
 * suggestion is reviewed, tested, and editable before an account is created.
 */
export interface AutoconfigResult {
  found: boolean;
  source: AutoconfigSource | null;
  /** Whether the provider prefers OAuth2 (a hint for the auth-method field). */
  oauth: boolean;
  displayName: string | null;
  incoming: AutoconfigIncoming | null;
  outgoing: AutoconfigOutgoing | null;
}

/**
 * Payload for the `update_account` command. Mirrors the Rust
 * `UpdateAccountInput`: it can change only the display name and each
 * server's host/port/security — never the protocol, username, or password.
 */
export interface UpdateAccountInput {
  accountId: number;
  displayName: string;
  incoming: ServerEndpointInput;
  outgoing: ServerEndpointInput;
}
