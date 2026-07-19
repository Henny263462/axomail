import type {
  AccountSummary,
  Pop3DeletePolicyDto,
  ThemePreference,
} from "../../types/mail";

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
  /** Current color-theme preference (batch-saved; previewed live on change). */
  readonly theme: ThemePreference;
  /** Whether displayed messages are auto-marked read (batch-saved). */
  readonly autoMarkReadEnabled: boolean;
  /** Delay before auto-marking a displayed message read, ms (batch-saved). */
  readonly autoMarkReadDelayMs: number;
  /** Stages a new notifications value for the shared Save. */
  setNotificationsEnabled(value: boolean): void;
  /** Stages a new sync-interval value for the shared Save. */
  setSyncIntervalMinutes(value: number): void;
  /** Stages the auto-mark-read flag and delay for the shared Save. */
  setAutoMarkRead(enabled: boolean, delayMs: number): void;
  /**
   * Stages a new theme for the shared Save and applies it immediately as a
   * live preview; the dialog reverts the preview if the user cancels.
   */
  setTheme(value: ThemePreference): void;
  /** Shows a message in the dialog's shared assertive alert region. */
  reportError(message: string): void;
  /** Shows a message in the dialog's shared polite status region. */
  reportStatus(message: string): void;
  /**
   * Signals that account rows changed (edited or deleted) so the app shell can
   * re-fetch its accounts and reconcile a now-missing selection. Bubbles from
   * the dialog through {@link DialogHost} to `App.vue`; sections that never
   * mutate accounts simply do not call it.
   */
  notifyAccountsChanged(): void;
}

/** Normalizes an unknown thrown value into a user-facing settings error. */
export function messageFrom(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Could not save settings.";
}
