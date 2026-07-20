/**
 * Typed wrappers over the junk (spam) Tauri commands.
 *
 * `markJunk` / `markNotJunk` train the per-account Bayesian classifier and move
 * the messages to / from the Junk folder, resolving to a {@link MoveOutcome}
 * whose `moves` drive the optimistic list splice (like the relocation actions).
 * `runJunkControls` re-scores a folder and returns how many messages it filed.
 * The settings pair reads and writes the junk controls. All mutations use
 * {@link invokeOrThrow} so a backend failure reaches the caller, while the
 * browser preview degrades to `null` (ADR-0005).
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type { JunkSettings, MoveOutcome } from "../types/mail";

/** Marks a selection of messages as junk (train + `$Junk` + move to Junk). */
export function markJunk(messageIds: number[]): Promise<MoveOutcome | null> {
  return invokeOrThrow<MoveOutcome>("mark_junk", { messageIds });
}

/** Marks a selection as not junk (train + clear `$Junk` + move to Inbox). */
export function markNotJunk(messageIds: number[]): Promise<MoveOutcome | null> {
  return invokeOrThrow<MoveOutcome>("not_junk", { messageIds });
}

/** Runs junk controls over a folder, returning how many messages were filed. */
export function runJunkControls(mailboxId: number): Promise<number | null> {
  return invokeOrThrow<number>("run_junk_controls", { mailboxId });
}

/** Reads the junk settings. `null` in the browser preview. */
export function getJunkSettings(): Promise<JunkSettings | null> {
  return safeInvoke<JunkSettings>("get_junk_settings");
}

/** Persists the junk settings. Rejects with the backend error on failure. */
export function setJunkSettings(settings: JunkSettings): Promise<null> {
  return invokeOrThrow<null>("set_junk_settings", { settings });
}
