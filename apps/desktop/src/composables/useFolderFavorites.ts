/**
 * Favorite-folder marking for the sidebar's "Favorite" view mode.
 *
 * Which folders are favorites is a pure client-side view preference (like the
 * pane layout and sizes), so it lives in `localStorage` keyed by the stable
 * local mailbox id — no schema column, no backend round-trip. A single
 * module-level reactive set is the source of truth so every consumer (the
 * context-menu toggle, the favorites list) stays in sync.
 *
 * A stale id (its folder was deleted or its account removed) simply never
 * matches a live folder, so it is harmless; the sidebar filters favorites
 * against the current account tree.
 */
import { computed, reactive, type ComputedRef } from "vue";

/** localStorage key holding the favorite mailbox ids as a JSON array. */
const STORAGE_KEY = "axomail.favoriteFolders";

/** The favorite mailbox ids (reactive; `has`/`add`/`delete` are tracked). */
const favorites = reactive(new Set<number>());
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
      for (const id of parsed) if (typeof id === "number") favorites.add(id);
    }
  } catch {
    // Missing or corrupt cache: start with no favorites.
  }
}

/** Persists the current set; a failed write is non-fatal. */
function persist(): void {
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, JSON.stringify([...favorites]));
  } catch {
    // A failed cache write only loses the change on the next launch.
  }
}

/** Favorite-folder state plus the toggle. */
export function useFolderFavorites(): {
  favoriteIds: ComputedRef<number[]>;
  isFavorite: (mailboxId: number) => boolean;
  toggleFavorite: (mailboxId: number) => void;
} {
  ensureLoaded();
  return {
    favoriteIds: computed(() => [...favorites]),
    isFavorite: (mailboxId) => favorites.has(mailboxId),
    toggleFavorite: (mailboxId) => {
      if (favorites.has(mailboxId)) favorites.delete(mailboxId);
      else favorites.add(mailboxId);
      persist();
    },
  };
}
