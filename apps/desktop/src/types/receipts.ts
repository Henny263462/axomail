/**
 * Read-receipt (MDN) view models.
 *
 * Split out of `mail.ts` (size gate). `ReceiptRequestDto` is imported by
 * `mail.ts` for the message-detail field; the rest are consumed directly by the
 * receipts settings section and the reading-pane bar via `../types/receipts`.
 * Mirrors the serialized Rust DTO shapes (camelCase).
 */

/** How incoming read-receipt requests are honoured. */
export type ReceiptPolicy = "never" | "ask" | "always";

/** The read-receipt honour policy plus its two conditions. */
export interface ReceiptSettings {
  policy: ReceiptPolicy;
  /** Only honour a request when the message is addressed to me. */
  onlyWhenAddressed: boolean;
  /** Never honour a request for mailing-list / bulk mail. */
  neverForMailingLists: boolean;
}

/**
 * A pending read-receipt request surfaced on a received message. Present only
 * when the message asked for a receipt, the policy decided to act, and none was
 * sent yet.
 */
export interface ReceiptRequestDto {
  /** Address the receipt would be sent to. */
  to: string;
  /** `"ask"` = show the request and wait; `"send"` = send automatically. */
  action: "ask" | "send";
}

/** Outcome slug of a `send_mdn` call. */
export type MdnStatus = "sent" | "alreadySent" | "suppressed" | "failed";

/** The result of sending a read receipt. */
export interface MdnOutcomeDto {
  sent: boolean;
  status: MdnStatus;
  error: string | null;
}
