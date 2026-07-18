/**
 * Single gateway to the Tauri IPC bridge.
 *
 * Every backend call in the frontend goes through `safeInvoke` so the
 * same build runs unchanged as a browser-served web preview (ADR-0005):
 * without the bridge, calls resolve to `null` instead of throwing.
 */
import { invoke } from "@tauri-apps/api/core";

interface TauriGlobals {
  __TAURI_INTERNALS__?: unknown;
}

/** True when running inside the Tauri desktop shell. */
export function isTauri(): boolean {
  return (globalThis as TauriGlobals).__TAURI_INTERNALS__ !== undefined;
}

/**
 * Invokes a Tauri command, or resolves to `null` when the IPC bridge is
 * unavailable (browser preview, tests) or the command fails.
 * Failures are logged for the diagnostics console, never thrown at
 * component level.
 */
export async function safeInvoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T | null> {
  if (!isTauri()) {
    return null;
  }
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    console.error(`invoke ${command} failed`, error);
    return null;
  }
}

/**
 * Invokes a Tauri command like {@link safeInvoke} but does **not** swallow
 * command failures: it resolves to `null` only when the IPC bridge is
 * absent (browser preview, tests), and otherwise rejects with the
 * backend's user-facing error string on failure.
 *
 * Use this for commands whose error message must reach the user — e.g.
 * account setup, where the backend reports why credentials were rejected.
 * The browser preview still degrades gracefully to `null` (ADR-0005); the
 * caller decides how to explain that the action needs the desktop app.
 */
export async function invokeOrThrow<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T | null> {
  if (!isTauri()) {
    return null;
  }
  return invoke<T>(command, args);
}
