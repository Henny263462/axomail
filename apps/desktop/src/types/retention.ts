/**
 * Per-mailbox retention-policy DTOs (track 5.5). Kept out of `types/mail.ts`,
 * which sits at the file-size gate; import these directly.
 *
 * The string unions mirror the Rust `RetentionMode` / `RetentionAction` serde
 * `snake_case` forms exactly, so the values must stay identical.
 */

/** Which messages a mailbox keeps. */
export type RetentionMode = "keep_all" | "by_age" | "by_count";

/** How the maintenance pass disposes of over-retention messages. */
export type RetentionAction = "trash" | "purge_body";

/** A mailbox's retention policy, as read from the backend. */
export interface RetentionPolicyDto {
  mailboxId: number;
  mode: RetentionMode;
  /** Age boundary in days (honoured for `by_age`). */
  days: number;
  /** Keep-count (honoured for `by_count`). */
  maxCount: number;
  action: RetentionAction;
}

/** The editable fields sent when saving a retention policy. */
export interface RetentionPolicyInput {
  mode: RetentionMode;
  days: number;
  maxCount: number;
  action: RetentionAction;
}

/** What a retention maintenance pass did. */
export interface RetentionOutcomeDto {
  movedToTrash: number;
  purgedBodies: number;
}
