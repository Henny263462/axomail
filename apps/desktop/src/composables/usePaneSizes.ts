/**
 * Resizable-pane sizes for the three-column shell: the folder-pane width, the
 * message-list width (used when the reading pane sits *beside* the list), and
 * the message-list height (used when the reading pane sits *below* the list).
 *
 * A single module-level source of truth (one window, one set of sizes) is
 * persisted in `localStorage`, so {@link initPaneSizes} can restore the widths
 * before the first frame (called from `main.ts`). Every value is clamped to a
 * sensible min/max so a drag or a corrupt cache can never wedge a pane to zero
 * or off-screen. Sizes are a pure UI preference — no backend round-trip.
 *
 * The module has no import-time side effects: nothing touches storage until
 * {@link initPaneSizes} or a setter runs.
 */
import { computed, reactive, type ComputedRef } from "vue";

/** The three resizable dimensions the splitters drive. */
export type PaneDimension = "sidebarWidth" | "listWidth" | "listHeight";

/** Inclusive pixel bounds per dimension. */
export const PANE_BOUNDS: Record<PaneDimension, { min: number; max: number }> = {
  sidebarWidth: { min: 160, max: 480 },
  listWidth: { min: 240, max: 720 },
  listHeight: { min: 140, max: 900 },
};

/** Defaults matching the original fixed layout (`tokens.css`). */
const DEFAULTS: Record<PaneDimension, number> = {
  sidebarWidth: 240,
  listWidth: 340,
  listHeight: 320,
};

/** localStorage key holding the persisted `{ dimension: px }` map. */
const STORAGE_KEY = "axomail.paneSizes";

/** The live sizes, seeded with the defaults until {@link initPaneSizes} runs. */
const sizes = reactive<Record<PaneDimension, number>>({ ...DEFAULTS });

/** Clamps `value` to the dimension's bounds, rounding to a whole pixel. */
function clamp(dimension: PaneDimension, value: number): number {
  const { min, max } = PANE_BOUNDS[dimension];
  if (!Number.isFinite(value)) return DEFAULTS[dimension];
  return Math.min(max, Math.max(min, Math.round(value)));
}

/** Persists the current sizes; a failed write is non-fatal. */
function persist(): void {
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, JSON.stringify(sizes));
  } catch {
    // A failed cache write only costs the restored sizes on the next launch.
  }
}

/** Restores persisted sizes (each clamped). Call once, as early as possible. */
export function initPaneSizes(): void {
  let stored: Partial<Record<PaneDimension, unknown>> = {};
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (raw) stored = JSON.parse(raw) as Partial<Record<PaneDimension, unknown>>;
  } catch {
    // Missing or corrupt cache: fall back to the defaults below.
  }
  for (const dimension of Object.keys(DEFAULTS) as PaneDimension[]) {
    const value = stored[dimension];
    sizes[dimension] =
      typeof value === "number" ? clamp(dimension, value) : DEFAULTS[dimension];
  }
}

/** Sets a dimension to an absolute pixel size (clamped) and persists it. */
export function setPaneSize(dimension: PaneDimension, value: number): void {
  sizes[dimension] = clamp(dimension, value);
  persist();
}

/** Nudges a dimension by `delta` pixels (clamped) and persists it. */
export function nudgePaneSize(dimension: PaneDimension, delta: number): void {
  setPaneSize(dimension, sizes[dimension] + delta);
}

/** Reactive pane sizes plus the bounds and mutation helpers. */
export function usePaneSizes(): {
  sizes: Record<PaneDimension, number>;
  bounds: typeof PANE_BOUNDS;
  cssVars: ComputedRef<Record<string, string>>;
  setPaneSize: (dimension: PaneDimension, value: number) => void;
  nudgePaneSize: (dimension: PaneDimension, delta: number) => void;
} {
  return {
    sizes,
    bounds: PANE_BOUNDS,
    // The custom properties the shell grid keys off; overrides the token
    // defaults so a resized pane wins.
    cssVars: computed(() => ({
      "--sidebar-width": `${sizes.sidebarWidth}px`,
      "--list-width": `${sizes.listWidth}px`,
      "--list-height": `${sizes.listHeight}px`,
    })),
    setPaneSize,
    nudgePaneSize,
  };
}
