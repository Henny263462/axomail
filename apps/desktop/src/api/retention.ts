/**
 * Typed wrappers over the per-mailbox retention commands (track 5.5).
 *
 * Reads use {@link safeInvoke} (resolve to `null` in the browser preview, where
 * the IPC bridge is absent — ADR-0005); mutations use {@link invokeOrThrow} so
 * a backend failure reaches the caller's error surface (still `null` in the
 * browser preview).
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type {
  RetentionOutcomeDto,
  RetentionPolicyDto,
  RetentionPolicyInput,
} from "../types/retention";

/** Reads a mailbox's retention policy. `null` in the browser preview. */
export function getFolderRetention(
  mailboxId: number,
): Promise<RetentionPolicyDto | null> {
  return safeInvoke<RetentionPolicyDto>("get_folder_retention", { mailboxId });
}

/**
 * Sets a mailbox's retention policy. Rejects with the backend's user-facing
 * error string on failure.
 */
export function setFolderRetention(
  mailboxId: number,
  policy: RetentionPolicyInput,
): Promise<null> {
  return invokeOrThrow<null>("set_folder_retention", { mailboxId, policy });
}

/**
 * Runs the retention maintenance pass over a mailbox once and reports what it
 * did (messages moved to Trash / bodies purged). Rejects with the backend's
 * error string on failure; resolves to `null` in the browser preview.
 */
export function applyFolderRetention(
  mailboxId: number,
): Promise<RetentionOutcomeDto | null> {
  return invokeOrThrow<RetentionOutcomeDto>("apply_folder_retention", {
    mailboxId,
  });
}
