import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  initTheme,
  reconcileTheme,
  setTheme,
  useTheme,
} from "../src/composables/useTheme";

const STORAGE_KEY = "axomail.theme";

/** A controllable `matchMedia` stand-in for the dark-mode query. */
interface MediaControl {
  /** Flips the OS preference and notifies listeners, as the OS would. */
  emit(dark: boolean): void;
}

const original = globalThis.matchMedia;

/**
 * Installs a fake `(prefers-color-scheme: dark)` media query seeded to
 * `initialDark`. Returns a handle that can fire live change events.
 */
function installMatchMedia(initialDark: boolean): MediaControl {
  const listeners = new Set<(event: MediaQueryListEvent) => void>();
  let matches = initialDark;
  const mql = {
    get matches() {
      return matches;
    },
    media: "(prefers-color-scheme: dark)",
    addEventListener(_type: string, cb: (event: MediaQueryListEvent) => void) {
      listeners.add(cb);
    },
    removeEventListener(
      _type: string,
      cb: (event: MediaQueryListEvent) => void,
    ) {
      listeners.delete(cb);
    },
  };
  globalThis.matchMedia = vi.fn(
    () => mql as unknown as MediaQueryList,
  ) as unknown as typeof globalThis.matchMedia;
  return {
    emit(dark: boolean) {
      matches = dark;
      for (const cb of listeners) cb({ matches: dark } as MediaQueryListEvent);
    },
  };
}

/** The value the tokens key off. */
function domTheme(): string | null {
  return document.documentElement.getAttribute("data-theme");
}

beforeEach(() => {
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  installMatchMedia(false);
});

afterEach(() => {
  globalThis.matchMedia = original;
});

describe("useTheme", () => {
  it("pins the light palette regardless of the OS preference", () => {
    installMatchMedia(true); // OS prefers dark…
    setTheme("light"); // …but the user pinned light.

    expect(domTheme()).toBe("light");
    expect(useTheme().resolvedTheme.value).toBe("light");
  });

  it("pins the dark palette regardless of the OS preference", () => {
    installMatchMedia(false); // OS prefers light…
    setTheme("dark"); // …but the user pinned dark.

    expect(domTheme()).toBe("dark");
    expect(useTheme().resolvedTheme.value).toBe("dark");
  });

  it("resolves the system preference to the current OS setting", () => {
    installMatchMedia(true);
    setTheme("system");
    expect(domTheme()).toBe("dark");

    installMatchMedia(false);
    setTheme("system");
    expect(domTheme()).toBe("light");
  });

  it("follows live prefers-color-scheme changes while on system", () => {
    const media = installMatchMedia(false);
    setTheme("system");
    expect(domTheme()).toBe("light");

    media.emit(true);
    expect(domTheme()).toBe("dark");

    media.emit(false);
    expect(domTheme()).toBe("light");
  });

  it("ignores OS changes once a fixed theme is pinned", () => {
    const media = installMatchMedia(false);
    setTheme("light");
    expect(domTheme()).toBe("light");

    // The OS goes dark, but the pinned light theme must not change.
    media.emit(true);
    expect(domTheme()).toBe("light");
  });

  it("paints the cached preference on init, before any backend read", () => {
    localStorage.setItem(STORAGE_KEY, "dark");
    installMatchMedia(false);

    initTheme();

    expect(domTheme()).toBe("dark");
    expect(useTheme().theme.value).toBe("dark");
  });

  it("caches the chosen preference for a no-flash next launch", () => {
    setTheme("dark");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("dark");

    setTheme("system");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("system");
  });

  it("treats a corrupt cached value as system", () => {
    localStorage.setItem(STORAGE_KEY, "solarized");
    installMatchMedia(false);

    initTheme();

    expect(useTheme().theme.value).toBe("system");
    expect(domTheme()).toBe("light");
  });

  it("falls back to light for system when matchMedia is unavailable", () => {
    installMatchMedia(false);
    initTheme(); // seeds systemDark = false via the fake

    // A host without matchMedia (e.g. a minimal SSR shell) must not throw.
    globalThis.matchMedia =
      undefined as unknown as typeof globalThis.matchMedia;
    expect(() => setTheme("system")).not.toThrow();
    expect(domTheme()).toBe("light");
  });
});

describe("startup theme reconcile race", () => {
  it("applies the backend theme when the user has not chosen this session", () => {
    initTheme(); // fresh, un-pinned session
    reconcileTheme("dark");

    expect(domTheme()).toBe("dark");
    expect(useTheme().theme.value).toBe("dark");
  });

  it("keeps a user-picked theme when a slow backend reconcile resolves late", () => {
    // The user opens Settings early and picks Dark before the slow getSettings
    // read (which still holds the old Light value) resolves.
    initTheme();
    setTheme("dark"); // user action in Settings
    reconcileTheme("light"); // stale backend value lands afterwards

    // The user's live choice must stand; the reconcile is a no-op.
    expect(domTheme()).toBe("dark");
    expect(useTheme().theme.value).toBe("dark");
  });

  it("re-arms the guard on a new session so a later reconcile applies again", () => {
    setTheme("dark"); // pin from a prior session
    initTheme(); // a new session starts un-pinned
    reconcileTheme("light");

    expect(domTheme()).toBe("light");
  });
});
