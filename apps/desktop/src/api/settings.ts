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
