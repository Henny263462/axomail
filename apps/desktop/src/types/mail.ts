/**
 * UI-facing view models.
 *
 * These mirror the serialized shapes of the Rust core models
 * (crates/mail-core) that future Tauri commands will return. They carry
 * only what list and detail views render — never bodies, raw MIME, or
 * credentials.
 *
 * ID precision invariant: Rust IDs are i64 (SQLite row keys) while JS
 * numbers are exact only up to 2^53-1. The command layer asserts that
 * every ID crossing the IPC boundary fits Number.MAX_SAFE_INTEGER and
 * fails loudly otherwise, so `number` is safe here.
 */

export interface MailboxSummary {
  id: number;
  name: string;
  role: "inbox" | "drafts" | "sent" | "trash" | "archive" | "junk" | "custom";
  unreadCount: number;
}

export interface AccountSummary {
  id: number;
  displayName: string;
  email: string;
  mailboxes: MailboxSummary[];
}

export interface MessageListItem {
  id: number;
  mailboxId: number;
  subject: string | null;
  fromName: string;
  fromAddress: string;
  /** Unix seconds (UTC); null when the date header was absent. */
  dateUnix: number | null;
  seen: boolean;
  flagged: boolean;
  /** Sanitized plain-text preview, no markup. */
  preview: string | null;
}

/** One page of message envelopes plus the cursor for the next page. */
export interface MessageListPage {
  items: MessageListItem[];
  /** Opaque cursor for the following page, or null on the last page. */
  nextCursor: string | null;
}

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

/**
 * Payload for the `create_account` command. Mirrors the Rust
 * `CreateAccountInput` exactly (serde `camelCase`).
 */
export interface CreateAccountInput {
  displayName: string;
  email: string;
  incoming: IncomingServerConfig;
  outgoing: OutgoingServerConfig;
}
