/**
 * Sidebar folder-view mode: which slice of folders the sidebar shows, mirroring
 * Thunderbird's folder-pane modes.
 *
 * - **all** — the full account folder trees plus the tag group (the original
 *   sidebar; the default, so behaviour is unchanged until the user switches).
 * - **unified** — only the cross-account "All inboxes" entry.
 * - **favorite** — only folders the user marked as favorites.
 * - **tags** — only the tag filter group.
 * - **recent** — only recently-opened folders.
 *
 * The mode is a pure client-side view preference persisted in `localStorage`.
 */
import { computed, ref, type ComputedRef } from "vue";

/** Which slice of folders the sidebar shows. */
export type FolderViewMode = "all" | "unified" | "favorite" | "tags" | "recent";

/** The modes in display order for the switcher. */
export const FOLDER_VIEW_MODES: readonly FolderViewMode[] = [
  "all",
  "unified",
  "favorite",
  "tags",
  "recent",
];

/** localStorage key holding the persisted mode. */
const STORAGE_KEY = "axomail.folderView";

/** The active mode; `all` preserves the original full sidebar. */
const mode = ref<FolderViewMode>("all");
let loaded = false;

/** Whether `value` names a known mode. */
function isMode(value: string | null): value is FolderViewMode {
  return (
    value === "all" ||
    value === "unified" ||
    value === "favorite" ||
    value === "tags" ||
    value === "recent"
  );
}

/** Loads the persisted mode once, on first use. */
function ensureLoaded(): void {
  if (loaded) return;
  loaded = true;
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY) ?? null;
    if (isMode(raw)) mode.value = raw;
  } catch {
    // Missing or corrupt cache: keep the default.
  }
}

/** Folder-view mode state plus the setter. */
export function useFolderView(): {
  mode: ComputedRef<FolderViewMode>;
  setMode: (next: FolderViewMode) => void;
} {
  ensureLoaded();
  return {
    mode: computed(() => mode.value),
    setMode: (next) => {
      mode.value = next;
      try {
        globalThis.localStorage?.setItem(STORAGE_KEY, next);
      } catch {
        // A failed cache write only loses the choice on the next launch.
      }
    },
  };
}
