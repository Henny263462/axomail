/**
 * Typed wrappers over the OpenPGP Tauri commands (Track 5.1).
 *
 * Reads use {@link safeInvoke} (resolving to `null` in the browser preview
 * where the IPC bridge is absent, ADR-0005); mutations use {@link invokeOrThrow}
 * so a backend failure reaches the caller's error surface. No secret key
 * material or passphrase is ever returned by a read — an own key exposes only
 * its public metadata.
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type {
  PgpComposeStatusDto,
  PgpKeyDto,
  PgpMessageStatusDto,
  PgpRecipientKeyDto,
} from "../types/pgp";

/** Lists our own keys. */
export function listOwnKeys(): Promise<PgpKeyDto[] | null> {
  return safeInvoke<PgpKeyDto[]>("pgp_list_own_keys");
}

/** Lists the recipient public keys we hold. */
export function listRecipientKeys(): Promise<PgpRecipientKeyDto[] | null> {
  return safeInvoke<PgpRecipientKeyDto[]>("pgp_list_recipient_keys");
}

/** Generates a new keypair, optionally passphrase-locked. */
export function generateKey(
  userId: string,
  passphrase: string | null,
): Promise<PgpKeyDto | null> {
  return invokeOrThrow<PgpKeyDto>("pgp_generate_key", { userId, passphrase });
}

/** Imports an existing armored secret key as one of our own. */
export function importSecretKey(armored: string): Promise<PgpKeyDto | null> {
  return invokeOrThrow<PgpKeyDto>("pgp_import_secret_key", { armored });
}

/** Imports a recipient's armored public key for an email address. */
export function importRecipientKey(
  email: string,
  armored: string,
): Promise<PgpRecipientKeyDto | null> {
  return invokeOrThrow<PgpRecipientKeyDto>("pgp_import_recipient_key", {
    email,
    armored,
  });
}

/** Exports the armored public key of one of our keys. */
export function exportPublicKey(id: number): Promise<string | null> {
  return invokeOrThrow<string>("pgp_export_public_key", { id });
}

/** Exports the armored secret key of one of our keys (user-initiated backup). */
export function exportSecretKey(id: number): Promise<string | null> {
  return invokeOrThrow<string>("pgp_export_secret_key", { id });
}

/** Makes one of our keys the default signing key. */
export function setDefaultKey(id: number): Promise<null> {
  return invokeOrThrow<null>("pgp_set_default_key", { id });
}

/** Deletes one of our keys and purges its keychain secret. */
export function deleteOwnKey(id: number): Promise<boolean | null> {
  return invokeOrThrow<boolean>("pgp_delete_own_key", { id });
}

/** Deletes a recipient public key. */
export function deleteRecipientKey(id: number): Promise<boolean | null> {
  return invokeOrThrow<boolean>("pgp_delete_recipient_key", { id });
}

/** Reports whether an outgoing message can be signed and/or encrypted. */
export function composeStatus(
  fromEmail: string,
  recipients: string[],
): Promise<PgpComposeStatusDto | null> {
  return safeInvoke<PgpComposeStatusDto>("pgp_compose_status", {
    fromEmail,
    recipients,
  });
}

/** Analyses a received message's OpenPGP status (verify + decrypt). */
export function analyzeMessagePgp(
  messageId: number,
  passphrase: string | null,
): Promise<PgpMessageStatusDto | null> {
  return safeInvoke<PgpMessageStatusDto>("analyze_message_pgp", {
    messageId,
    passphrase,
  });
}
