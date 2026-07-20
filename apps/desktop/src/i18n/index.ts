/**
 * Vue I18n framework instance and the app-language preference plumbing.
 *
 * Wave 5 Stage 0 sets up the framework so the Wave-5 UI tracks — and the full
 * 5.4 extraction — can emit translatable strings. This is the scaffolding, not
 * the extraction: English is the source locale and the default, so any
 * component not yet converted to `t()` / `$t` renders exactly as before.
 *
 * ## How a component translates (the 5.4 recipe)
 *
 * - Template: `{{ $t("settings.appearance.title") }}` — `$t` is globally
 *   injected (`globalInjection: true`), so no import is needed.
 * - Script setup: `const { t } = useI18n({ useScope: "global" });` then
 *   `t("settings.appearance.title")`. Always pass `useScope: "global"` so the
 *   component reads the shared catalogue rather than an empty per-component
 *   scope. See `AppearanceSection.vue` for the worked example.
 *
 * ## Where strings live
 *
 * `locales/en.json` (the source of truth) and `locales/de.json`. Keys are
 * dot-namespaced by area (`settings.appearance.title`, `language.system`). Add
 * a key to `en.json` first, then mirror it in `de.json`; a missing `de` key
 * falls back to English (`fallbackLocale`).
 *
 * ## The language preference
 *
 * `LanguagePreference` is `system | en | de`. `system` resolves against the
 * OS/browser language (`navigator.language`) at apply time, defaulting to
 * English. The authoritative value persists in the backend `app_settings`
 * store (`get_language` / `set_language`, see `api/settings.ts`); a
 * `localStorage` cache lets {@link initLanguage} paint the right locale before
 * the first frame, and `App.vue` reconciles the cache with the backend on
 * startup — the same flow `useTheme` uses for the color theme.
 */
import { createI18n } from "vue-i18n";

import de from "./locales/de.json";
import en from "./locales/en.json";

/** The concrete locales that ship with the app. English is the source. */
export const SUPPORTED_LOCALES = ["en", "de"] as const;

/** A concrete UI locale (a real translation catalogue). */
export type AppLocale = (typeof SUPPORTED_LOCALES)[number];

/**
 * The user's language preference. `system` follows the OS/browser language
 * (resolved to a concrete {@link AppLocale} at apply time); `en` / `de` pin
 * one. Mirrors the Rust `LanguagePreference` and its `app_settings` key.
 */
export type LanguagePreference = "system" | AppLocale;

/** localStorage key holding the cached preference for no-flash startup. */
const STORAGE_KEY = "axomail.language";

/**
 * The shared Vue I18n instance, installed via `app.use(i18n)` in `main.ts`.
 * Composition mode (`legacy: false`) with global `$t` injection. The message
 * compiler is retained (Vite keeps `__INTLIFY_DROP_MESSAGE_COMPILER__` false),
 * so plain JSON catalogues compile just-in-time at runtime.
 */
export const i18n = createI18n({
  legacy: false,
  globalInjection: true,
  locale: "en",
  fallbackLocale: "en",
  messages: { en, de },
});

/**
 * Whether the user has explicitly chosen a language this session. Once true,
 * the startup backend reconcile ({@link reconcileLanguage}) must not override
 * the user's live choice — the same race guard `useTheme` uses.
 */
let userPinned = false;

/** Resolves a preference to a concrete locale, mapping `system` to the OS. */
export function resolveLocale(preference: LanguagePreference): AppLocale {
  if (preference === "en" || preference === "de") return preference;
  const system = globalThis.navigator?.language?.toLowerCase() ?? "en";
  return system.startsWith("de") ? "de" : "en";
}

/** Reads the cached preference, treating anything unexpected as `system`. */
function readStored(): LanguagePreference {
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (raw === "en" || raw === "de" || raw === "system") return raw;
  } catch {
    // localStorage can throw when disabled (private mode, sandbox); ignore.
  }
  return "system";
}

/** Caches the preference; a failed write only costs a flash on next launch. */
function writeStored(value: LanguagePreference): void {
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, value);
  } catch {
    // Non-fatal: the cache is only a no-flash optimization.
  }
}

/** Applies a preference to the active locale and updates the cache. */
function apply(preference: LanguagePreference): void {
  i18n.global.locale.value = resolveLocale(preference);
  writeStored(preference);
}

/**
 * Initializes the locale from the cache. Call once, before the app mounts, so
 * the first frame paints in the user's language (no English→German flip).
 */
export function initLanguage(): void {
  // A fresh session starts un-pinned so the upcoming backend reconcile is free
  // to apply the authoritative value.
  userPinned = false;
  apply(readStored());
}

/**
 * Sets the language in response to a user action (the settings section),
 * pinning it so a late startup reconcile cannot override the live choice.
 */
export function setLanguagePreference(preference: LanguagePreference): void {
  userPinned = true;
  apply(preference);
}

/**
 * Applies the authoritative backend preference during startup reconciliation,
 * unless the user already picked one this session (then this is a no-op).
 */
export function reconcileLanguage(preference: LanguagePreference): void {
  if (userPinned) return;
  apply(preference);
}
