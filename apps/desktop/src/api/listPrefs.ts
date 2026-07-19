/**
 * Typed wrappers over the message-list view-preference commands.
 *
 * The threaded/flat toggle is a single persisted view preference owned by the
 * list UI (track 3.4), kept out of the batch-saved {@link SettingsDto} so it
 * has its own read/write pair. Reads resolve to `null` in the browser preview
 * where the IPC bridge is absent (ADR-0005), letting the caller fall back to
 * the flat default.
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";

/** Reads whether the message list groups messages into threads (default flat). */
export function getListThreaded(): Promise<boolean | null> {
  return safeInvoke<boolean>("get_list_threaded");
}

/**
 * Persists the threaded/flat list preference. Uses {@link invokeOrThrow} so a
 * backend failure rejects (the caller can revert its optimistic toggle);
 * resolves `null` in the browser preview, where the toggle simply stands.
 */
export function setListThreaded(threaded: boolean): Promise<null> {
  return invokeOrThrow<null>("set_list_threaded", { threaded });
}
