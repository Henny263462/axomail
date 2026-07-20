import { computed, onMounted, onUnmounted, reactive } from "vue";

import { subscribePushStatus } from "../api/push";
import type { PushConnState, PushStatusDto } from "../types/mail";

/**
 * Subscribes to the `axomail://push` channel for the lifetime of the calling
 * component and tracks the IMAP IDLE connection state of each account, so the
 * UI can show "connected (push)" versus polling.
 *
 * State is per account: an account is "connected" only while an IDLE
 * connection is watching its inbox; a drop moves it to "reconnecting" and an
 * account whose server has no IDLE (or has push disabled) reports "polling".
 * Accounts never heard from are simply absent (treated as polling by callers).
 *
 * The subscription is disposed on unmount, including when the component
 * unmounts before the async subscription resolves.
 */
export function usePushStatus() {
  // accountId -> latest push state.
  const states = reactive(new Map<number, PushConnState>());

  let dispose: (() => void) | null = null;
  let disposed = false;

  function handle(event: PushStatusDto): void {
    states.set(event.accountId, event.state);
  }

  onMounted(async () => {
    try {
      const off = await subscribePushStatus(handle);
      if (disposed) off();
      else dispose = off;
    } catch {
      // Push status is a best-effort UI accent; a failed subscription must
      // never break the status bar it lives in.
    }
  });

  onUnmounted(() => {
    disposed = true;
    dispose?.();
  });

  /** The push state for one account (`"polling"` when never reported). */
  function pushStateFor(accountId: number): PushConnState {
    return states.get(accountId) ?? "polling";
  }

  /** True when at least one account has a live push connection. */
  const anyConnected = computed(() => {
    for (const state of states.values()) {
      if (state === "connected") return true;
    }
    return false;
  });

  return { pushStateFor, anyConnected };
}
