/**
 * Typed wrapper over the `discover_autoconfig` Tauri command.
 *
 * Given an email address, the backend looks up the provider's incoming and
 * outgoing server settings (Mozilla ISPDB, the domain's own autoconfig, or a
 * common-provider guess) over HTTPS. No credentials are sent — only the email
 * domain drives the lookup — and nothing is persisted: the result pre-fills the
 * setup form for the user to review, test, and edit.
 */
import { safeInvoke } from "../lib/tauri";
import type { AutoconfigResult } from "../types/mail";

/**
 * Discovers server settings for an email address.
 *
 * Resolves to the lookup outcome, or to `null` in the browser preview where
 * the IPC bridge is absent (ADR-0005) or the command could not run — the
 * caller then invites manual entry. A successful lookup that found nothing
 * resolves to a result with `found: false`.
 */
export function discoverAutoconfig(
  email: string,
): Promise<AutoconfigResult | null> {
  return safeInvoke<AutoconfigResult>("discover_autoconfig", { email });
}
