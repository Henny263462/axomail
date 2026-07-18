/**
 * Typed wrapper over the `list_unified_inbox` Tauri command.
 *
 * The unified inbox merges every account's inbox-role mailbox into one
 * newest-first stream. Resolves to `null` in the browser preview where the
 * IPC bridge is absent (ADR-0005).
 */
import { safeInvoke } from "../lib/tauri";
import type { UnifiedPageDto } from "../types/mail";

/**
 * Loads one page of the unified inbox, newest first.
 *
 * @param cursor Opaque cursor from a previous page, or `null` for the first.
 * @param limit Maximum number of envelopes to return.
 */
export function listUnifiedInbox(
  cursor: string | null,
  limit: number,
): Promise<UnifiedPageDto | null> {
  return safeInvoke<UnifiedPageDto>("list_unified_inbox", { cursor, limit });
}
