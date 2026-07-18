/**
 * Typed wrapper over the `test_connection` Tauri command.
 *
 * The command attempts a live login to the incoming and outgoing servers
 * described by a {@link CreateAccountInput} and reports each result
 * independently. Nothing is persisted and the supplied secrets go nowhere
 * but the live connection attempt.
 */
import { safeInvoke } from "../lib/tauri";
import type { ConnectionTestResult, CreateAccountInput } from "../types/mail";

/**
 * Tests the incoming and outgoing servers of a prospective account.
 *
 * Resolves to the per-server result, or to `null` in the browser preview
 * where the IPC bridge is absent (ADR-0005) — the caller explains that the
 * test needs the desktop app.
 */
export function testConnection(
  input: CreateAccountInput,
): Promise<ConnectionTestResult | null> {
  return safeInvoke<ConnectionTestResult>("test_connection", { input });
}
