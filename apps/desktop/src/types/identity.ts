/**
 * Sender-identity view models (composer From-picker, signatures, and
 * composition preferences).
 *
 * Split out of `mail.ts` (barrel) to keep each file under the size gate.
 * Mirrors the Rust `IdentityDto` / `IdentityInput` (serde `camelCase`).
 */

/** Where a signature sits relative to quoted text in a reply. */
export type SignaturePlacement = "below_quote" | "above_quote";

/** Whether a reply body is typed above or below the quote. */
export type ReplyPosition = "above" | "below";

/** How a forwarded message carries the original. */
export type ForwardMode = "inline" | "attachment";

/** Archive sub-folder granularity. */
export type ArchiveGranularity = "flat" | "yearly" | "monthly";

/** One sender identity, as returned by `list_identities`. */
export interface IdentityDto {
  id: number;
  accountId: number;
  email: string;
  displayName: string;
  replyTo: string | null;
  signatureText: string | null;
  signaturePlacement: SignaturePlacement;
  replyPosition: ReplyPosition;
  forwardMode: ForwardMode;
  archiveGranularity: ArchiveGranularity;
  isDefault: boolean;
  sortPosition: number;
  sentDest: number | null;
  draftsDest: number | null;
  templatesDest: number | null;
  archiveDest: number | null;
}

/**
 * Payload for `create_identity`/`update_identity`. Mirrors the Rust
 * `IdentityInput` (serde `camelCase`); mailbox destinations are optional and
 * consumed by a later wave.
 */
export interface IdentityInput {
  accountId: number;
  email: string;
  displayName: string;
  replyTo: string | null;
  signatureText: string | null;
  signaturePlacement: SignaturePlacement;
  replyPosition: ReplyPosition;
  forwardMode: ForwardMode;
  archiveGranularity: ArchiveGranularity;
  makeDefault: boolean;
  sentDest?: number | null;
  draftsDest?: number | null;
  templatesDest?: number | null;
  archiveDest?: number | null;
}
