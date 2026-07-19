/**
 * Typed wrappers over the OAuth2 sign-in and account connection commands.
 *
 * `oauthAuthorize` runs the RFC 8252 browser sign-in for an already-created
 * OAuth2 account and stores its tokens in the OS keychain; only a status ever
 * crosses back. Each call resolves to `null` in the browser preview where the
 * IPC bridge is absent (ADR-0005).
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type { ConnectionTestResult, OAuthAuthorizeResult } from "../types/mail";

/**
 * Runs the system-browser sign-in for an OAuth2 account and stores the
 * resulting tokens. Resolves with the outcome (`authorized`, `denied`, or
 * `timedOut`), or rejects with the backend's user-facing error string on a
 * hard failure (e.g. the token exchange failed). Resolves to `null` in the
 * browser preview.
 */
export function oauthAuthorize(
  accountId: number,
): Promise<OAuthAuthorizeResult | null> {
  return invokeOrThrow<OAuthAuthorizeResult>("oauth_authorize", { accountId });
}

/**
 * Tests connectivity to a stored account's servers, resolving credentials
 * from the keychain (refreshing an OAuth2 token when due). Resolves to `null`
 * in the browser preview.
 */
export function testAccountConnection(
  accountId: number,
): Promise<ConnectionTestResult | null> {
  return safeInvoke<ConnectionTestResult>("test_account_connection", {
    accountId,
  });
}
