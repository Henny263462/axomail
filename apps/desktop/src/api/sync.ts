/**
 * Typed wrappers over the sync command and the sync event channel.
 *
 * `syncAccount` enqueues background jobs; progress is reported out-of-band
 * as `axomail://sync` Tauri events, which {@link subscribeSyncEvents}
 * delivers to a handler. Both degrade gracefully outside the desktop shell
 * (ADR-0005): the command resolves to `null`, and the subscription is a
 * no-op that still returns a disposer so callers need no branching.
 */
import { listen } from "@tauri-apps/api/event";

import { isTauri, safeInvoke } from "../lib/tauri";
import type { SyncEventDto } from "../types/mail";

/** Tauri event name carrying {@link SyncEventDto} payloads. */
const SYNC_EVENT = "axomail://sync";

/**
 * Enqueues a sync job for every syncable mailbox of the account and
 * resolves to the new job ids. Resolves to `null` in the browser preview.
 */
export function syncAccount(accountId: number): Promise<number[] | null> {
  return safeInvoke<number[]>("sync_account", { accountId });
}

/**
 * Subscribes to sync progress events, invoking `handler` for each one.
 *
 * Resolves to a disposer that unsubscribes; call it on unmount. Outside the
 * desktop shell the bridge is absent, so this subscribes to nothing and the
 * disposer is a no-op — the handler is simply never called.
 */
export async function subscribeSyncEvents(
  handler: (event: SyncEventDto) => void,
): Promise<() => void> {
  if (!isTauri()) {
    return () => {};
  }
  return listen<SyncEventDto>(SYNC_EVENT, (event) => handler(event.payload));
}
