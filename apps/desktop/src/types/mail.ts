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
