import type { AccountSummary, Pop3DeletePolicyDto } from "../../types/mail";

/**
 * Shared, read-through context handed to every settings section by
 * {@link SettingsDialog}. Each section receives it as its single `context`
 * prop, which keeps the section registry uniform: registering a new section
 * is one new SFC plus one entry in `sections.ts`.
 *
 * Sections never mutate the context. The two "general" fields
 * (`notificationsEnabled`, `syncIntervalMinutes`) are batch-saved by the
 * dialog's shared Save button, so a section reads them and pushes edits back
 * through the `set*` callbacks. Autonomous sections (e.g. POP3 deletion)
 * persist through their own api call and surface the outcome via
 * `reportError` / `reportStatus`, which drive the dialog's shared aria-live
 * regions.
 */
export interface SettingsSectionContext {
  /** Accounts, used by sections that key off them (POP3 delete policy). */
  readonly accounts: AccountSummary[];
  /** POP3 delete policies as loaded; the dialog echoes them back on save. */
  readonly initialPolicies: Pop3DeletePolicyDto[];
  /** Current "show desktop notifications" value (batch-saved). */
  readonly notificationsEnabled: boolean;
  /** Current sync interval in minutes (batch-saved; floor of 1). */
  readonly syncIntervalMinutes: number;
  /** Stages a new notifications value for the shared Save. */
  setNotificationsEnabled(value: boolean): void;
  /** Stages a new sync-interval value for the shared Save. */
  setSyncIntervalMinutes(value: number): void;
  /** Shows a message in the dialog's shared assertive alert region. */
  reportError(message: string): void;
  /** Shows a message in the dialog's shared polite status region. */
  reportStatus(message: string): void;
}

/** Normalizes an unknown thrown value into a user-facing settings error. */
export function messageFrom(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Could not save settings.";
}
