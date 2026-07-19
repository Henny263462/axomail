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

/** One addressee of a message (display name optional). */
export interface RecipientDto {
  name: string | null;
  address: string;
}

/** Metadata for a single attachment; the bytes are not carried here. */
export interface AttachmentMetaDto {
  filename: string;
  mimeType: string;
  size: number;
}

/**
 * One deceptive-link ("scam") warning for the reading pane. Warning only:
 * the sanitized body is never altered.
 */
export interface ScamFlagDto {
  /** Stable machine kind: "deceptiveLink" or "ipAddressHost". */
  kind: string;
  /** Human-readable reason shown in the warning banner. */
  detail: string;
}

/**
 * Full detail of one message, including the sanitized viewing payload.
 *
 * `textBody` and `sanitizedHtml` are the only body-bearing fields the
 * frontend ever holds, and only for the message currently open.
 */
export interface MessageDetailDto {
  id: number;
  subject: string | null;
  fromName: string;
  fromAddress: string;
  to: RecipientDto[];
  cc: RecipientDto[];
  /** Unix seconds (UTC); null when the date header was absent. */
  dateUnix: number | null;
  textBody: string | null;
  /** Sanitized HTML for the sandboxed iframe, or null when absent. */
  sanitizedHtml: string | null;
  /**
   * Count of remote resources blocked during sanitization. When remote
   * images are allowed this counts only the still-blocked data:/srcset.
   */
  remoteBlockedCount: number;
  /** Whether remote http(s) images were kept for this render. */
  remoteAllowed: boolean;
  /** Deceptive-link warnings found in the sanitized DOM (warning only). */
  scamFlags: ScamFlagDto[];
  /** True when scripts/forms/other active content were stripped. */
  hadActiveContent: boolean;
  attachments: AttachmentMetaDto[];
  /** Message-ID without angle brackets, or null when the message had none. */
  messageId: string | null;
  /** In-Reply-To message-ids in header order (usually one parent). */
  inReplyTo: string[];
  /** References chain, oldest ancestor first. */
  references: string[];
}

/**
 * Payload for `send_message` and `save_draft`. Recipient lists are already
 * split and trimmed by the composer before crossing the IPC boundary.
 */
export interface ComposeInput {
  accountId: number;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  textBody: string;
}

/** A saved draft, as returned by `list_drafts`. */
export interface DraftDto {
  id: number;
  accountId: number;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  textBody: string;
  updatedUnix: number;
}

/** Delivery lifecycle state of an outbox item. */
export type OutboxState = "pending" | "sending" | "sent" | "failed";

/** One queued or delivered message in the outbox. */
export interface OutboxItemDto {
  id: number;
  accountId: number;
  subject: string;
  toSummary: string;
  state: OutboxState;
  attempts: number;
  lastError: string | null;
  updatedUnix: number;
}

/**
 * Outcome of a send or retry attempt. `sent` is true only when the message
 * left the machine; otherwise it sits in the outbox and `error` explains why.
 */
export interface SendOutcomeDto {
  outboxId: number;
  sent: boolean;
  error: string | null;
}

/**
 * One full-text search hit for the results view. `snippet` carries the
 * FTS `[`/`]` markers around matched terms and MUST be rendered as plain
 * text (the markers stripped or styled), never as HTML.
 */
export interface SearchResultDto {
  messageId: number;
  mailboxId: number;
  accountId: number;
  subject: string | null;
  fromName: string;
  fromAddress: string;
  /** Unix seconds (UTC); null when the date header was absent. */
  dateUnix: number | null;
  snippet: string;
}

/**
 * A unified-inbox envelope: the same shape a mailbox list item has, tagged
 * with the account it belongs to so the list can show an account indicator.
 */
export interface UnifiedItemDto extends MessageListItem {
  accountId: number;
}

/** One page of the unified inbox plus the cursor for the next page. */
export interface UnifiedPageDto {
  items: UnifiedItemDto[];
  /** Opaque cursor for the following page, or null on the last page. */
  nextCursor: string | null;
}

/** Per-POP3-account server-deletion policy. */
export interface Pop3DeletePolicyDto {
  accountId: number;
  enabled: boolean;
}

/**
 * Preferred application color theme. `system` follows the OS light/dark
 * setting; `light` and `dark` pin one. Mirrors the Rust `ThemePreference`
 * enum (serde `lowercase`), so the string values must stay identical.
 */
export type ThemePreference = "system" | "light" | "dark";

/**
 * User-editable application settings. `syncIntervalMinutes` has a minimum of
 * 1; the backend clamps it, and the settings UI enforces the same floor.
 */
export interface SettingsDto {
  notificationsEnabled: boolean;
  syncIntervalMinutes: number;
  theme: ThemePreference;
  pop3DeletePolicies: Pop3DeletePolicyDto[];
}

/** Lifecycle events emitted per sync job on the `axomail://sync` channel. */
export type SyncEventKind =
  | "jobQueued"
  | "jobStarted"
  | "batchIngested"
  | "jobFinished"
  | "jobCancelled"
  | "jobFailed";

/**
 * A sync progress event. The shape is flat (a `kind` discriminant plus
 * nullable detail fields) to mirror the backend's serialized payload
 * exactly; only the fields relevant to `kind` are populated.
 */
export interface SyncEventDto {
  kind: SyncEventKind;
  jobId: number;
  mailboxId: number | null;
  newMessages: number | null;
  errorClass: string | null;
  message: string | null;
}

/** Kind of work a background job represents (mirrors the Rust `JobKind`). */
export type SyncJobKind = "discover" | "mailboxSync" | "fetch" | "send";

/** Lifecycle state of a background job (mirrors the Rust `JobState`). */
export type SyncJobState =
  | "pending"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled";

/**
 * One background sync job as shown in the activity manager. `accountLabel`
 * and `mailboxLabel` are the human-readable names resolved by the backend;
 * either is null when its owning row no longer exists (e.g. a deleted
 * account). No message content is ever carried here.
 */
export interface SyncJobDto {
  id: number;
  kind: SyncJobKind;
  state: SyncJobState;
  accountId: number | null;
  accountLabel: string | null;
  mailboxId: number | null;
  mailboxLabel: string | null;
  /** Redacted failure reason, present only for failed jobs. */
  error: string | null;
  /** Unix seconds when the job was claimed, or null if never started. */
  startedAtUnix: number | null;
  /** Unix seconds when the job reached a terminal state, or null. */
  finishedAtUnix: number | null;
}
