/**
 * "Send later" scheduling helpers: quick presets and Unix-second conversions.
 *
 * All computations are in the user's local time zone (the composer shows local
 * clock times) and return whole Unix seconds, the unit the backend schedules
 * on. Kept pure so the presets and parsing can be unit-tested without a DOM.
 */

/** A quick-pick delay offered by the send-later menu. */
export interface SendLaterPreset {
  /** Stable identifier for keys and tests. */
  id: string;
  /** Human-readable label. */
  label: string;
  /** Target Unix second computed from a base time in milliseconds. */
  at: (nowMs: number) => number;
}

/** Floors a millisecond instant to whole Unix seconds. */
function unixSeconds(ms: number): number {
  return Math.floor(ms / 1000);
}

/** Local clock time `hour:00` on the day `addDays` from `nowMs`. */
function atLocalHour(nowMs: number, addDays: number, hour: number): number {
  const date = new Date(nowMs);
  date.setDate(date.getDate() + addDays);
  date.setHours(hour, 0, 0, 0);
  return date.getTime();
}

/** Local clock time `hour:00` on the next Monday strictly after `nowMs`. */
function nextMonday(nowMs: number, hour: number): number {
  const date = new Date(nowMs);
  const day = date.getDay(); // 0 = Sunday … 6 = Saturday
  const daysUntilMonday = (8 - day) % 7 || 7; // always 1..7, never today
  date.setDate(date.getDate() + daysUntilMonday);
  date.setHours(hour, 0, 0, 0);
  return date.getTime();
}

/** The quick-pick send-later presets, in menu order. */
export const SEND_LATER_PRESETS: SendLaterPreset[] = [
  { id: "in-1-hour", label: "In 1 hour", at: (now) => unixSeconds(now + 3_600_000) },
  { id: "in-3-hours", label: "In 3 hours", at: (now) => unixSeconds(now + 10_800_000) },
  {
    id: "tomorrow-morning",
    label: "Tomorrow, 8:00",
    at: (now) => unixSeconds(atLocalHour(now, 1, 8)),
  },
  {
    id: "monday-morning",
    label: "Monday, 8:00",
    at: (now) => unixSeconds(nextMonday(now, 8)),
  },
];

/**
 * Parses a `<input type="datetime-local">` value (local time, no zone) into
 * whole Unix seconds, or `null` when the value is empty or unparseable.
 */
export function datetimeLocalToUnix(value: string): number | null {
  if (value.length === 0) return null;
  const ms = new Date(value).getTime();
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000);
}

/** Formats a Unix second as a localized date-and-time for display. */
export function formatScheduleTime(unix: number): string {
  return new Date(unix * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}
