/**
 * Presentation helpers shared by the message-detail components.
 *
 * These are pure formatting utilities — no data fetching, no side effects —
 * kept separate so the view components stay focused on markup.
 */
import type { RecipientDto } from "../../types/mail";

const SIZE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/**
 * Formats a byte count as a short human-readable size (e.g. `12.3 KB`).
 * Negative or non-finite inputs collapse to `0 B`.
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const exponent = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    SIZE_UNITS.length - 1,
  );
  const value = bytes / 1024 ** exponent;
  const rounded = exponent === 0 ? value : Math.round(value * 10) / 10;
  return `${rounded} ${SIZE_UNITS[exponent]}`;
}

/**
 * Formats a Unix-seconds timestamp as a locale date-and-time string, or an
 * empty string when the date header was absent.
 */
export function formatDateTime(unix: number | null): string {
  if (unix === null) return "";
  return new Date(unix * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

/**
 * Renders a recipient as `Name <address>` (or just the address when the
 * display name is absent).
 */
export function formatRecipient(recipient: RecipientDto): string {
  const address = recipient.address;
  const name = recipient.name?.trim();
  return name ? `${name} <${address}>` : address;
}

/** Joins a recipient list into a single comma-separated string. */
export function formatRecipients(recipients: RecipientDto[]): string {
  return recipients.map(formatRecipient).join(", ");
}
