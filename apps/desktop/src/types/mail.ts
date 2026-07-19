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
 *
 * Account-setup/auth and sender-identity view models live in the sibling
 * `account.ts` / `identity.ts` modules (size gate) and are re-exported here so
 * `../types/mail` stays the single import surface.
 */

import type { AuthMechanism } from "./account";

export * from "./account";
export * from "./identity";

export interface MailboxSummary {
  id: number;
  /** Full decoded folder path; the tree derives a leaf from it + delimiter. */
  name: string;
  role: "inbox" | "drafts" | "sent" | "trash" | "archive" | "junk" | "custom";
  unreadCount: number;
  /**
   * The following fields are always populated by the backend DTO. They are
   * optional here only so lightweight test fixtures may omit them; the folder
   * tree normalizes any missing value to a safe default.
   */
  /** Containing folder id, or null for a top-level folder. */
  parentId?: number | null;
  /** Server hierarchy delimiter for this folder, or null when flat. */
  delimiter?: string | null;
  /** Whether the folder is subscribed. */
  subscribed?: boolean;
  /** Whether the folder vanished from the server but still holds messages. */
  orphaned?: boolean;
  /** Total number of messages in the folder. */
  totalCount?: number;
}

export interface AccountSummary {
  id: number;
  displayName: string;
  email: string;
  /** `password` or `oauth2`; drives whether "Sign in with browser" applies. */
  authMechanism: AuthMechanism;
  mailboxes: MailboxSummary[];
}

/** A compact tag chip (colored dot + label) shown on rows and the detail. */
export interface TagChip {
  id: number;
  displayName: string;
  /** Presentation color as `#rrggbb`, or null for colorless. */
  color: string | null;
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
  /**
   * Threading identity, used by the list to assemble conversation threads
   * client-side. Always populated by the backend DTO; optional here only so
   * lightweight test fixtures may omit them (thread assembly treats a missing
   * value as "no threading headers", i.e. a singleton thread).
   */
  /** Own Message-ID without angle brackets, or null when absent. */
  messageId?: string | null;
  /** In-Reply-To message-ids in header order (usually one parent). */
  inReplyTo?: string[];
  /** References chain, oldest ancestor first. */
  references?: string[];
  /** Tag chips for this row, in display order (empty when untagged). */
  tags: TagChip[];
}

/** A full tag record for the tag manager (keyword + usage count). */
export interface TagDto {
  id: number;
  accountId: number;
  keyword: string;
  displayName: string;
  color: string | null;
  sortOrder: number;
  usageCount: number;
}

/** One option in the per-message tag picker. */
export interface MessageTagOption {
  id: number;
  displayName: string;
  color: string | null;
  applied: boolean;
}

/** The tag picker payload for one message: its account plus every option. */
export interface MessageTagOptions {
  messageId: number;
  accountId: number;
  options: MessageTagOption[];
}

/** One page of message envelopes plus the cursor for the next page. */
export interface MessageListPage {
  items: MessageListItem[];
  /** Opaque cursor for the following page, or null on the last page. */
  nextCursor: string | null;
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
  /** Whether the message is starred (authoritative state for the pane star). */
  flagged: boolean;
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
  /**
   * True only for the offline placeholder: the body is not cached and could
   * not be fetched because the app is working offline. Header fields are still
   * populated; the body fields are empty. Absent (falsey) on a normal load.
   */
  offlineUnavailable?: boolean;
}

/**
 * Payload for `send_message` and `save_draft`. Recipient lists are already
 * split and trimmed by the composer before crossing the IPC boundary.
 */
export interface ComposeInput {
  accountId: number;
  /** Chosen sender identity; drives From/Reply-To when present. */
  identityId?: number | null;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  textBody: string;
  /** Unix second to send at, for a "send later" message. */
  scheduledAtUnix?: number | null;
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
  /** Identity the message will be sent under, if any. */
  identityId?: number | null;
  /** Unix second a scheduled ("send later") message is withheld until. */
  scheduledUnix?: number | null;
  updatedUnix: number;
}

/**
 * Outcome of a send, retry, or schedule attempt. `sent` is true only when the
 * message left the machine; otherwise it sits in the outbox and `error`
 * explains why — unless `scheduledAtUnix` is set, meaning it was queued for
 * later (no error).
 */
export interface SendOutcomeDto {
  outboxId: number;
  sent: boolean;
  error: string | null;
  /** When set, the message was queued to send at this Unix second. */
  scheduledAtUnix?: number | null;
  /**
   * True when the message was queued because the app is working offline; it
   * stays in the outbox and is sent automatically on returning online.
   */
  queuedOffline?: boolean;
}

/**
 * How much of a mailbox is kept available offline. Mirrors the Rust
 * `OfflineMode` (serde `snake_case`), so the string values must stay identical.
 */
export type OfflineMode = "none" | "recent" | "all";

/** A mailbox's offline download policy, shown in folder properties. */
export interface OfflinePolicyDto {
  mailboxId: number;
  mode: OfflineMode;
  /** Age boundary (days) honoured only in `recent` mode. */
  recentDays: number;
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
  /** Whether a displayed message is marked read automatically. */
  autoMarkReadEnabled: boolean;
  /** Delay before auto-marking a displayed message read (ms; 0 = immediate). */
  autoMarkReadDelayMs: number;
  pop3DeletePolicies: Pop3DeletePolicyDto[];
}

/** Lifecycle events emitted per sync job on the `axomail://sync` channel. */
export type SyncEventKind =
  | "jobQueued"
  | "jobStarted"
  | "batchIngested"
  | "jobFinished"
  | "opsReplayed"
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
  /** Account the event concerns (write-back `opsReplayed` events). */
  accountId: number | null;
  newMessages: number | null;
  /** Operations replayed to the server (`opsReplayed` events). */
  replayed: number | null;
  /** Operations parked as failed (`opsReplayed` events). */
  failed: number | null;
  errorClass: string | null;
  message: string | null;
}

/** Kind of work a background job represents (mirrors the Rust `JobKind`). */
export type SyncJobKind =
  | "discover"
  | "mailboxSync"
  | "fetch"
  | "send"
  | "opReplay";

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
