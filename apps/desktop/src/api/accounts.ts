/**
 * Typed wrappers over the account Tauri commands.
 *
 * Each call resolves to its DTO in the desktop shell, or to `null` in the
 * browser preview where the IPC bridge is absent (ADR-0005). `createAccount`
 * additionally rejects with the backend's user-facing error string on
 * failure so the setup dialog can show it (see {@link invokeOrThrow}).
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type { AccountSummary, CreateAccountInput } from "../types/mail";

/** Lists every configured account with its mailboxes. */
export function listAccounts(): Promise<AccountSummary[] | null> {
  return safeInvoke<AccountSummary[]>("list_accounts");
}

/**
 * Creates an account, its default local mailboxes, and its keychain
 * secrets, returning the new account summary.
 *
 * Resolves to `null` in the browser preview (no backend). Rejects with the
 * backend's user-facing error string when the desktop backend refuses the
 * configuration.
 */
export function createAccount(
  input: CreateAccountInput,
): Promise<AccountSummary | null> {
  return invokeOrThrow<AccountSummary>("create_account", { input });
}

/**
 * Deletes an account, cascading its database rows and removing both
 * keychain secrets. Resolves to `null` in the browser preview.
 */
export function deleteAccount(accountId: number): Promise<null> {
  return safeInvoke<null>("delete_account", { accountId });
}
