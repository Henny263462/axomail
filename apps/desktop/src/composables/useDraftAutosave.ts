import { onBeforeUnmount, onMounted } from "vue";

/**
 * Behaviour the composer supplies so its in-progress content can be saved to a
 * local draft on a timer and once more when the dialog closes.
 */
export interface DraftAutosaveOptions {
  /** Save cadence in milliseconds. Defaults to 30s. */
  intervalMs?: number;
  /**
   * A stable string fingerprint of the persisted fields (recipients, subject,
   * body). Autosave runs only when it changes, so an untouched draft is never
   * rewritten and the timer does no needless work.
   */
  snapshot: () => string;
  /** True when there is nothing worth saving (a blank composer). */
  isEmpty: () => boolean;
  /** True while a send or manual save is in flight; autosave defers to it. */
  isBusy: () => boolean;
  /** True once the message was sent, so the close flush must not resurrect it. */
  isFinished: () => boolean;
  /** Performs the actual draft save. */
  save: () => Promise<void>;
}

/**
 * Periodically saves the composer to a local draft, and flushes once more when
 * the component unmounts (the dialog closing) — unless the message was sent.
 *
 * All the tricky guards live here so the composer shell stays declarative: an
 * empty composer is never saved, a save is skipped while a manual send/save is
 * running, and an unchanged snapshot is never rewritten (so reopening a draft
 * and closing it again does nothing). A sent message is never re-saved: its
 * draft is cleaned up by the send flow instead.
 */
export function useDraftAutosave(options: DraftAutosaveOptions): void {
  const intervalMs = options.intervalMs ?? 30_000;
  let lastSaved = options.snapshot();
  let timer: ReturnType<typeof setInterval> | null = null;

  async function flush(): Promise<void> {
    if (options.isBusy() || options.isFinished() || options.isEmpty()) return;
    const snapshot = options.snapshot();
    if (snapshot === lastSaved) return;
    lastSaved = snapshot;
    await options.save();
  }

  onMounted(() => {
    timer = setInterval(() => void flush(), intervalMs);
  });

  onBeforeUnmount(() => {
    if (timer !== null) clearInterval(timer);
    // Best-effort save on close; Vue does not await this hook, which is fine.
    void flush();
  });
}
