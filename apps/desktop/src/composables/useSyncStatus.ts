import { computed, onMounted, onUnmounted, reactive, ref } from "vue";

import { subscribeSyncEvents } from "../api/sync";
import type { SyncEventDto } from "../types/mail";

/**
 * Subscribes to the `axomail://sync` event channel for the lifetime of the
 * calling component and tracks which mailboxes are actively syncing plus
 * the most recent job failure.
 *
 * The optional `onEvent` callback receives every event after internal state
 * is updated, letting the host react (e.g. reload a mailbox on ingest) while
 * this composable owns the running-set and error bookkeeping. The
 * subscription is disposed on unmount, including when the component unmounts
 * before the async subscription resolves.
 *
 * No message bodies are retained here — only job ids, mailbox ids, and a
 * redacted failure message.
 */
export function useSyncStatus(onEvent?: (event: SyncEventDto) => void) {
  // jobId -> mailboxId (null when the job is not mailbox-scoped).
  const runningJobs = reactive(new Map<number, number | null>());
  const syncError = ref<string | null>(null);

  let dispose: (() => void) | null = null;
  let disposed = false;

  function handle(event: SyncEventDto): void {
    switch (event.kind) {
      case "jobQueued":
      case "jobStarted":
        runningJobs.set(event.jobId, event.mailboxId);
        break;
      case "jobFinished":
      case "jobFailed":
        runningJobs.delete(event.jobId);
        break;
      case "batchIngested":
        break;
    }
    if (event.kind === "jobFailed") {
      syncError.value = event.message ?? "A sync job failed.";
    }
    onEvent?.(event);
  }

  onMounted(async () => {
    const off = await subscribeSyncEvents(handle);
    if (disposed) off();
    else dispose = off;
  });

  onUnmounted(() => {
    disposed = true;
    dispose?.();
  });

  /** Mailbox ids with at least one job currently running. */
  const syncingMailboxIds = computed(() => {
    const ids = new Set<number>();
    for (const mailboxId of runningJobs.values()) {
      if (mailboxId !== null) ids.add(mailboxId);
    }
    return ids;
  });

  /** True while any sync job is running. */
  const isSyncing = computed(() => runningJobs.size > 0);

  /** Clears the surfaced failure message. */
  function dismissError(): void {
    syncError.value = null;
  }

  return { syncingMailboxIds, isSyncing, syncError, dismissError };
}
