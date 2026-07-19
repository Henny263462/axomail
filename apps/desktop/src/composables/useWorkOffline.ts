/**
 * Shared Work Offline state.
 *
 * The flag is genuinely cross-cutting — the status bar owns the toggle and its
 * indicator, while the outbox list reads it to label queued sends — so it lives
 * in a single module-level ref rather than in `App.vue`. Components call
 * {@link useWorkOffline} and observe the same reactive value.
 *
 * The backend is the source of truth (it also gates the sync engine and
 * scheduler and persists the setting); {@link load} pulls the current value and
 * {@link setOffline}/{@link toggle} push a change through the command, updating
 * the shared ref optimistically so the UI reacts immediately.
 */
import { readonly, ref } from "vue";

import { getWorkOffline, setWorkOffline } from "../api/offline";

const offline = ref(false);

export function useWorkOffline() {
  /** Loads the persisted Work Offline state from the backend. */
  async function load(): Promise<void> {
    const value = await getWorkOffline();
    if (value !== null) offline.value = value;
  }

  /** Sets the Work Offline mode, updating the shared flag optimistically. */
  async function setOffline(next: boolean): Promise<void> {
    const previous = offline.value;
    offline.value = next;
    try {
      await setWorkOffline(next);
    } catch (error) {
      // Roll back on failure so the indicator never lies about the real state.
      offline.value = previous;
      throw error;
    }
  }

  /** Flips the current Work Offline state. */
  async function toggle(): Promise<void> {
    await setOffline(!offline.value);
  }

  return { offline: readonly(offline), load, setOffline, toggle };
}
