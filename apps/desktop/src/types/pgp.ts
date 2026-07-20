/**
 * OpenPGP DTO types (Track 5.1), mirroring the Rust `dto::pgp` shapes
 * (camelCase). Kept in their own module so `types/mail.ts` — which sits near
 * the file-size gate — is untouched.
 */

/** One of our own OpenPGP keys (public metadata; secret is keychain-held). */
export interface PgpKeyDto {
  id: number;
  fingerprint: string;
  keyId: string;
  userId: string;
  algorithm: string;
  hasPassphrase: boolean;
  isDefault: boolean;
  createdAtUnix: number;
}

/** A recipient / contact public key used to encrypt outgoing mail. */
export interface PgpRecipientKeyDto {
  id: number;
  email: string;
  fingerprint: string;
  keyId: string;
  userId: string;
  algorithm: string;
  createdAtUnix: number;
}

/** Whether an outgoing message can be signed and/or encrypted. */
export interface PgpComposeStatusDto {
  canSign: boolean;
  canEncrypt: boolean;
  missingRecipients: string[];
}

/**
 * Signature verification outcome label for a received message.
 *
 * `signerMismatch` is the guarded outcome the backend emits when a signature is
 * cryptographically valid but made with a key that is NOT the sender's — it must
 * never be shown as a plain green "valid" badge (see {@link PgpMessageStatusDto.signer}).
 */
export type PgpSignatureStatus =
  | "none"
  | "valid"
  | "invalid"
  | "unknownKey"
  | "signerMismatch";

/** The OpenPGP security status of a received message, for the reading pane. */
export interface PgpMessageStatusDto {
  isPgp: boolean;
  encrypted: boolean;
  signed: boolean;
  signatureStatus: PgpSignatureStatus;
  signer: string | null;
  decrypted: boolean;
  needsPassphrase: boolean;
  bodyHtml: string | null;
  bodyText: string | null;
  error: string | null;
}
