/**
 * Typed wrappers over the offline-store Tauri commands: the global Work Offline
 * toggle, per-mailbox offline policy, an immediate "download now", and a
 * cache-cleanup purge.
 *
 * Reads use {@link safeInvoke} (resolve to `null` in the browser preview, where
 * the IPC bridge is absent — ADR-0005); mutations use {@link invokeOrThrow} so
 * a backend failure reaches the caller's error surface, still resolving to
 * `null` in the browser preview.
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type { OfflineMode, OfflinePolicyDto } from "../types/mail";

/** Reads the current Work Offline mode. `null` in the browser preview. */
export function getWorkOffline(): Promise<boolean | null> {
  return safeInvoke<boolean>("get_work_offline");
}

/**
 * Sets the global Work Offline mode. While offline no network jobs run, reads
 * serve from the local store, and sends queue in the outbox. Rejects with the
 * backend's user-facing error string on failure.
 */
export function setWorkOffline(offline: boolean): Promise<null> {
  return invokeOrThrow<null>("set_work_offline", { offline });
}

/** Reads a mailbox's offline policy. `null` in the browser preview. */
export function getMailboxOfflinePolicy(
  mailboxId: number,
): Promise<OfflinePolicyDto | null> {
  return safeInvoke<OfflinePolicyDto>("get_mailbox_offline_policy", {
    mailboxId,
  });
}

/**
 * Sets a mailbox's offline policy. Rejects with the backend's user-facing
 * error string on failure.
 */
export function setMailboxOfflinePolicy(
  mailboxId: number,
  mode: OfflineMode,
  recentDays: number,
): Promise<null> {
  return invokeOrThrow<null>("set_mailbox_offline_policy", {
    mailboxId,
    mode,
    recentDays,
  });
}

/**
 * Enqueues an immediate full offline download for a mailbox, regardless of the
 * polling interval or standing policy. Progress is observable through the
 * normal sync events. Rejects with the backend's error string on failure.
 */
export function downloadMailboxNow(mailboxId: number): Promise<null> {
  return invokeOrThrow<null>("download_mailbox_now", { mailboxId });
}

/**
 * Purges cached bodies that fall outside a mailbox's offline window, returning
 * how many messages were purged (or `null` in the browser preview).
 */
export function cleanupOfflineCache(mailboxId: number): Promise<number | null> {
  return safeInvoke<number>("cleanup_offline_cache", { mailboxId });
}
