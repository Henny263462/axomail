/**
 * Recently-used folder tracking for the sidebar's "Recent" view mode.
 *
 * The most-recently-opened mailbox ids, newest first, capped and persisted in
 * `localStorage`. A pure client-side view preference keyed by the stable local
 * mailbox id — no backend round-trip. A single module-level list keeps every
 * consumer in sync; stale ids never match a live folder and are harmless.
 */
import { computed, ref, type ComputedRef } from "vue";

/** localStorage key holding the recent mailbox ids (newest first). */
const STORAGE_KEY = "axomail.recentFolders";

/** How many recent folders to remember. */
const MAX_RECENT = 12;

/** The recent mailbox ids, newest first. */
const recent = ref<number[]>([]);
let loaded = false;

/** Loads the persisted ids once, on first use. */
function ensureLoaded(): void {
  if (loaded) return;
  loaded = true;
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (!raw) return;
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      recent.value = parsed.filter((id): id is number => typeof id === "number");
    }
  } catch {
    // Missing or corrupt cache: start with an empty history.
  }
}

/** Persists the current list; a failed write is non-fatal. */
function persist(): void {
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, JSON.stringify(recent.value));
  } catch {
    // A failed cache write only loses the change on the next launch.
  }
}

/** Recently-used folder state plus the record helper. */
export function useRecentFolders(): {
  recentIds: ComputedRef<number[]>;
  recordFolder: (mailboxId: number) => void;
} {
  ensureLoaded();
  return {
    recentIds: computed(() => recent.value),
    recordFolder: (mailboxId) => {
      recent.value = [
        mailboxId,
        ...recent.value.filter((id) => id !== mailboxId),
      ].slice(0, MAX_RECENT);
      persist();
    },
  };
}
