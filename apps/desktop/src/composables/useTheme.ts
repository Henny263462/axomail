/**
 * Application color-theme control.
 *
 * A single module-level source of truth (there is only ever one theme for the
 * window) drives the `data-theme` attribute on the document root, which the
 * design tokens key off (`styles/tokens.css`). The preference is one of
 * `system | light | dark`; `system` is resolved against the OS
 * `prefers-color-scheme` with a live media-query listener so the app follows
 * the OS in real time.
 *
 * The chosen preference is cached in `localStorage` so {@link initTheme} can
 * paint the correct theme before the first frame (called from `main.ts`
 * ahead of mounting), avoiding a light-to-dark flash. The authoritative value
 * still lives in the backend settings; `App.vue` reconciles the cache with it
 * on startup via {@link setTheme}, and the settings dialog persists changes.
 *
 * The module has no import-time side effects: nothing touches the DOM, storage,
 * or `matchMedia` until {@link initTheme} or {@link setTheme} runs. That keeps
 * it inert for components (and their tests) that merely import {@link setTheme}.
 */
import { computed, ref, type ComputedRef } from "vue";

import type { ThemePreference } from "../types/mail";

/** localStorage key holding the cached preference for no-flash startup. */
const STORAGE_KEY = "axomail.theme";
/** Media query used to resolve the `system` preference. */
const DARK_QUERY = "(prefers-color-scheme: dark)";

/** The concrete palette actually applied to the document. */
export type ResolvedTheme = "light" | "dark";

/** The user's chosen preference (defaults to following the OS). */
const preference = ref<ThemePreference>("system");
/** Whether the OS currently prefers a dark palette. */
const systemDark = ref(false);

/** Live handle to the dark-mode media query, or null before setup/in SSR. */
let mediaQuery: MediaQueryList | null = null;
/** The attached listener, tracked so setup can replace it idempotently. */
let mediaListener: ((event: MediaQueryListEvent) => void) | null = null;

/**
 * The palette in effect: the preference itself when pinned, or the OS
 * preference when following the system.
 */
const resolvedTheme: ComputedRef<ResolvedTheme> = computed(() =>
  preference.value === "system"
    ? systemDark.value
      ? "dark"
      : "light"
    : preference.value,
);

/** Read-only view of the current preference for consumers. */
const theme: ComputedRef<ThemePreference> = computed(() => preference.value);

/** Reads the cached preference, treating anything unexpected as `system`. */
function readStored(): ThemePreference {
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (raw === "light" || raw === "dark" || raw === "system") return raw;
  } catch {
    // localStorage can throw when disabled (private mode, sandbox); ignore.
  }
  return "system";
}

/** Caches the preference; failures to write are non-fatal. */
function writeStored(value: ThemePreference): void {
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, value);
  } catch {
    // A failed cache write only costs a one-frame flash on next launch.
  }
}

/** Reflects the resolved palette onto the document root for the tokens. */
function applyToDom(): void {
  const root = globalThis.document?.documentElement;
  if (root) root.setAttribute("data-theme", resolvedTheme.value);
}

/**
 * (Re)binds the `prefers-color-scheme` listener and seeds {@link systemDark}
 * from its current value. Safe to call repeatedly: any prior listener is
 * detached first. A no-op where `matchMedia` is unavailable (e.g. SSR).
 */
function bindMediaQuery(): void {
  if (typeof globalThis.matchMedia !== "function") return;
  if (mediaQuery && mediaListener) {
    mediaQuery.removeEventListener("change", mediaListener);
  }
  mediaQuery = globalThis.matchMedia(DARK_QUERY);
  systemDark.value = mediaQuery.matches;
  mediaListener = (event: MediaQueryListEvent): void => {
    systemDark.value = event.matches;
    applyToDom();
  };
  // `addEventListener` on MediaQueryList is universal in the target WebViews;
  // the guard keeps the seeded value working if a host omits it.
  if (typeof mediaQuery.addEventListener === "function") {
    mediaQuery.addEventListener("change", mediaListener);
  }
}

/**
 * Initializes theming from the cache and paints it immediately. Call once,
 * as early as possible (before the Vue app mounts), to avoid a theme flash.
 */
export function initTheme(): void {
  preference.value = readStored();
  bindMediaQuery();
  applyToDom();
}

/**
 * Sets and applies the preference, updating the no-flash cache. This is the
 * single commit path used by both the startup backend reconciliation and the
 * settings dialog.
 */
export function setTheme(next: ThemePreference): void {
  preference.value = next;
  writeStored(next);
  bindMediaQuery();
  applyToDom();
}

/** Reactive theme state plus the commit helpers. */
export function useTheme(): {
  theme: ComputedRef<ThemePreference>;
  resolvedTheme: ComputedRef<ResolvedTheme>;
  setTheme: (next: ThemePreference) => void;
  initTheme: () => void;
} {
  return { theme, resolvedTheme, setTheme, initTheme };
}
