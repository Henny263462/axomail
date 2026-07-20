/**
 * Typed wrappers over the settings Tauri commands.
 *
 * `getSettings` reads the current values and resolves to `null` in the
 * browser preview where the IPC bridge is absent (ADR-0005). The two
 * mutations use {@link invokeOrThrow} so a backend failure reaches the
 * settings dialog (which surfaces it via an aria-live region); they still
 * resolve to `null` in the browser preview.
 *
 * `setPop3DeletePolicy` is deliberately its own command: it is the ONLY way
 * to enable server-side deletion for a POP3 account (the account-setup dialog
 * never sets it). This explicit, per-account call is the user-visible second
 * guard against silently deleting mail from the server.
 */
import type { LanguagePreference } from "../i18n";
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type { SettingsDto } from "../types/mail";

/** Reads the current application settings. `null` in the browser preview. */
export function getSettings(): Promise<SettingsDto | null> {
  return safeInvoke<SettingsDto>("get_settings");
}

/**
 * Persists notification and sync-interval settings. Does not, by itself,
 * enable POP3 server deletion — that requires {@link setPop3DeletePolicy}.
 * Rejects with the backend's user-facing error string on failure.
 */
export function updateSettings(settings: SettingsDto): Promise<null> {
  return invokeOrThrow<null>("update_settings", { settings });
}

/**
 * Enables or disables server-side deletion after download for one POP3
 * account. Rejects with the backend's user-facing error string on failure.
 */
export function setPop3DeletePolicy(
  accountId: number,
  enabled: boolean,
): Promise<null> {
  return invokeOrThrow<null>("set_pop3_delete_policy", { accountId, enabled });
}

/**
 * Reads whether IMAP IDLE push is enabled (default `true`). `null` in the
 * browser preview.
 */
export function getPushEnabled(): Promise<boolean | null> {
  return safeInvoke<boolean>("get_push_enabled");
}

/**
 * Enables or disables IMAP IDLE push. When on, the app keeps an IDLE
 * connection open per account that supports it, so new mail arrives without
 * waiting for the next poll; when off, every account polls. Rejects with the
 * backend's user-facing error string on failure.
 */
export function setPushEnabled(enabled: boolean): Promise<null> {
  return invokeOrThrow<null>("set_push_enabled", { enabled });
}

/**
 * Reads the persisted UI-language preference (default `"system"`). `null` in
 * the browser preview. Like push and threading, the language lives outside
 * {@link SettingsDto} as its own read/write pair (it is applied entirely in
 * the frontend by the i18n layer) rather than widening the batch-saved surface.
 */
export function getLanguage(): Promise<LanguagePreference | null> {
  return safeInvoke<LanguagePreference>("get_language");
}

/**
 * Persists the UI-language preference. Rejects with the backend's user-facing
 * error string on failure. The frontend applies the locale change separately
 * via the i18n layer; this call only records the authoritative value.
 */
export function setLanguage(preference: LanguagePreference): Promise<null> {
  return invokeOrThrow<null>("set_language", { preference });
}
