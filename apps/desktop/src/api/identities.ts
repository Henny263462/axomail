/**
 * Typed wrappers over the identity Tauri commands.
 *
 * Reads resolve to `null` in the browser preview where the IPC bridge is
 * absent (ADR-0005). The mutations use {@link invokeOrThrow} so a backend
 * rejection (for example an invalid address) reaches the settings editor,
 * which surfaces it via an aria-live region; they still resolve to `null` in
 * the browser preview.
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type { IdentityDto, IdentityInput } from "../types/mail";

/** Lists every identity across all accounts. `null` in the browser preview. */
export function listIdentities(): Promise<IdentityDto[] | null> {
  return safeInvoke<IdentityDto[]>("list_identities");
}

/**
 * Creates a new identity for an account, returning it. Rejects with the
 * backend's user-facing error string on failure (e.g. an invalid address).
 */
export function createIdentity(
  input: IdentityInput,
): Promise<IdentityDto | null> {
  return invokeOrThrow<IdentityDto>("create_identity", { input });
}

/**
 * Updates an identity's content (and default flag when `makeDefault` is set),
 * returning the refreshed identity. Rejects with the backend's error string.
 */
export function updateIdentity(
  identityId: number,
  input: IdentityInput,
): Promise<IdentityDto | null> {
  return invokeOrThrow<IdentityDto>("update_identity", { identityId, input });
}

/** Deletes an identity. Resolves to `null` in the browser preview. */
export function deleteIdentity(identityId: number): Promise<null> {
  return safeInvoke<null>("delete_identity", { identityId });
}

/**
 * Marks an identity as its account's default. Rejects with the backend's
 * error string on failure.
 */
export function setDefaultIdentity(identityId: number): Promise<null> {
  return invokeOrThrow<null>("set_default_identity", { identityId });
}
