/**
 * Window-layout control: the three-pane arrangement of the mail shell,
 * mirroring Thunderbird's Classic / Vertical / Wide layouts.
 *
 * - **vertical** — folder pane, message list, and reading pane as three
 *   side-by-side columns (the default and the app's original arrangement).
 * - **classic** — folder pane on the left (full height); the message list sits
 *   above the reading pane on the right.
 * - **wide** — the message list spans the full width across the top; the folder
 *   pane and reading pane sit below it.
 *
 * A single module-level source of truth (there is only ever one layout for the
 * window) is persisted in `localStorage`, so {@link initLayout} can restore the
 * choice before the first frame (called from `main.ts`). The value is a pure UI
 * preference — no backend round-trip — so there is no reconcile step.
 *
 * The module has no import-time side effects: nothing touches storage until
 * {@link initLayout} or a setter runs, keeping it inert for unit tests that only
 * import the setters.
 */
import { computed, ref, type ComputedRef } from "vue";

/** The three-pane arrangement of the shell. */
export type PaneLayout = "classic" | "vertical" | "wide";

/** The layouts in cycle order (for a "next layout" command). */
export const PANE_LAYOUTS: readonly PaneLayout[] = ["classic", "vertical", "wide"];

/** localStorage key holding the persisted layout. */
const STORAGE_KEY = "axomail.layout";

/** The active layout; `vertical` preserves the app's original three columns. */
const layout = ref<PaneLayout>("vertical");

/** Whether `value` names a known layout. */
function isLayout(value: string | null): value is PaneLayout {
  return value === "classic" || value === "vertical" || value === "wide";
}

/** Reads the persisted layout, treating anything unexpected as `vertical`. */
function readStored(): PaneLayout {
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY) ?? null;
    if (isLayout(raw)) return raw;
  } catch {
    // localStorage can throw when disabled (private mode, sandbox); ignore.
  }
  return "vertical";
}

/** Persists the layout; a failed write is non-fatal. */
function writeStored(value: PaneLayout): void {
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, value);
  } catch {
    // A failed cache write only costs the restored layout on the next launch.
  }
}

/** Restores the persisted layout. Call once, as early as possible. */
export function initLayout(): void {
  layout.value = readStored();
}

/** Commits and persists a layout in response to a user action. */
export function setLayout(next: PaneLayout): void {
  layout.value = next;
  writeStored(next);
}

/** Advances to the next layout in {@link PANE_LAYOUTS} (the View command). */
export function cycleLayout(): void {
  const index = PANE_LAYOUTS.indexOf(layout.value);
  const next = PANE_LAYOUTS[(index + 1) % PANE_LAYOUTS.length];
  setLayout(next ?? "vertical");
}

/** Reactive layout state plus the commit helpers. */
export function useLayout(): {
  layout: ComputedRef<PaneLayout>;
  setLayout: (next: PaneLayout) => void;
  cycleLayout: () => void;
} {
  return {
    layout: computed(() => layout.value),
    setLayout,
    cycleLayout,
  };
}
