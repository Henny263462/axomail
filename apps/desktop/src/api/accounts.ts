/**
 * Typed wrappers over the account Tauri commands.
 *
 * Each call resolves to its DTO in the desktop shell, or to `null` in the
 * browser preview where the IPC bridge is absent (ADR-0005). `createAccount`
 * additionally rejects with the backend's user-facing error string on
 * failure so the setup dialog can show it (see {@link invokeOrThrow}).
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type {
  AccountDetail,
  AccountSummary,
  CreateAccountInput,
  UpdateAccountInput,
} from "../types/mail";

/** Lists every configured account with its mailboxes. */
export function listAccounts(): Promise<AccountSummary[] | null> {
  return safeInvoke<AccountSummary[]>("list_accounts");
}

/**
 * Lists every configured account with its editable server configuration
 * (no secrets). Backs the account-management settings section. Resolves to
 * `null` in the browser preview where the IPC bridge is absent (ADR-0005).
 */
export function listAccountDetails(): Promise<AccountDetail[] | null> {
  return safeInvoke<AccountDetail[]>("list_account_details");
}

/**
 * Updates an account's display name and both servers' host/port/security,
 * returning the refreshed detail. The protocol, login names, and stored
 * password are never changed by this call.
 *
 * Resolves to `null` in the browser preview (no backend). Rejects with the
 * backend's user-facing error string when the configuration is invalid or
 * the account no longer exists.
 */
export function updateAccount(
  input: UpdateAccountInput,
): Promise<AccountDetail | null> {
  return invokeOrThrow<AccountDetail>("update_account", { input });
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
 * Deletes an account, cascading its database rows and removing both keychain
 * secrets.
 *
 * Resolves to `null` in the browser preview (no backend). Rejects with the
 * backend's user-facing error string on failure so the confirmation dialog
 * can keep the account and explain why the deletion did not happen — a silent
 * failure on a destructive action would be misleading.
 */
export function deleteAccount(accountId: number): Promise<null> {
  return invokeOrThrow<null>("delete_account", { accountId });
}
