/**
 * Typed wrapper over the IMAP IDLE push status channel.
 *
 * Push connection state arrives out-of-band on the `axomail://push` Tauri
 * event channel (separate from the job-centric `axomail://sync` stream, which
 * is keyed by job id). This lives in its own module so components in the app
 * shell can subscribe without other api mocks needing to know about it. It
 * degrades gracefully outside the desktop shell (ADR-0005): the subscription
 * is a no-op that still returns a disposer, so callers need no branching.
 */
import { listen } from "@tauri-apps/api/event";

import { isTauri } from "../lib/tauri";
import type { PushStatusDto } from "../types/mail";

/** Tauri event name carrying {@link PushStatusDto} payloads. */
const PUSH_EVENT = "axomail://push";

/**
 * Subscribes to IMAP IDLE push status changes, invoking `handler` for each.
 *
 * Resolves to a disposer that unsubscribes; call it on unmount. Outside the
 * desktop shell the bridge is absent, so this subscribes to nothing and the
 * disposer is a no-op — the handler is simply never called.
 */
export async function subscribePushStatus(
  handler: (event: PushStatusDto) => void,
): Promise<() => void> {
  if (!isTauri()) {
    return () => {};
  }
  return listen<PushStatusDto>(PUSH_EVENT, (event) => handler(event.payload));
}
