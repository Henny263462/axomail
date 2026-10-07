/**
 * Compose / reply / outbox view models.
 *
 * Split out of `mail.ts` (size gate) and re-exported from it, so `../types/mail`
 * stays the single import surface. Mirrors the serialized shapes of the Rust
 * compose DTOs (camelCase); ids are `number` (safe under the IPC id invariant
 * documented in `mail.ts`).
 */

/**
 * Payload for `send_message` and `save_draft`. Recipient lists are already
 * split and trimmed by the composer before crossing the IPC boundary.
 */
export interface ComposeInput {
  accountId: number;
  /** Chosen sender identity; drives From/Reply-To when present. */
  identityId?: number | null;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  textBody: string;
  /**
   * Sanitized HTML alternative body. Sent as the `multipart/alternative` HTML
   * part on an immediate send; not persisted (the durable draft/outbox is
   * text-only), so a deferred send falls back to `textBody`.
   */
  htmlBody?: string | null;
  /**
   * Absolute paths of files to attach, chosen through the native open-file
   * dialog. The backend reads the bytes with `std::fs` at send time; the
   * webview never reads file content. Attachments ride only on an immediate
   * send.
   */
  attachments?: string[];
  /** Unix second to send at, for a "send later" message. */
  scheduledAtUnix?: number | null;
  /** `In-Reply-To` header value (source Message-ID) when this is a reply. */
  inReplyTo?: string | null;
  /** `References` chain (oldest first) when this is a reply. */
  references?: string[];
  /**
   * Inline attachments carried as base64 content rather than a filesystem path
   * — today a forwarded message round-tripped as a `message/rfc822` part (from
   * the reply/forward prefill). Combined server-side with the picked-file
   * {@link attachments} into one `multipart/mixed`; immediate-send only.
   */
  forwardedAttachments?: ComposeAttachmentDto[];
  /**
   * OpenPGP-sign this message (Track 5.1). Immediate-send only; a deferred send
   * that requests it is rejected by the backend.
   */
  sign?: boolean;
  /** OpenPGP-encrypt this message to its recipients. Immediate-send only. */
  encrypt?: boolean;
  /**
   * Passphrase to unlock the signing key for this send only. Never persisted or
   * stored beyond the keychain-held secret key.
   */
  pgpPassphrase?: string | null;
  /**
   * When true, request a read receipt (MDN): the send path writes a
   * `Disposition-Notification-To` header pointing at the sender's address.
   * Immediate-send only (dropped on a scheduled or offline send).
   */
  requestReceipt?: boolean;
}

/**
 * An attachment carried by a composed message. Content is base64-encoded to
 * survive the IPC boundary. Produced today by the reply/forward prefill.
 */
export interface ComposeAttachmentDto {
  filename: string;
  mimeType: string;
  contentBase64: string;
}

/** Which reply operation a composer prefill is prepared for. */
export type ReplyMode =
  | "reply"
  | "replyAll"
  | "forward"
  | "forwardInline"
  | "forwardAttachment";

export type ForwardStyle = "inline" | "attachment";

/** Field seed for a fresh composition (used by "New message from template"). */
export interface ComposeInitial {
  to?: string[];
  cc?: string[];
  bcc?: string[];
  subject?: string;
  body?: string;
}

/** How a source message is marked once its reply/forward is sent. */
export type SourceMark = "answered" | "forwarded";

/** The source message a reply/forward came from, and how to mark it on send. */
export interface PrefillSourceDto {
  messageId: number;
  mark: SourceMark;
}

/**
 * Everything the composer needs to open pre-filled for a reply or forward,
 * returned by `prepare_reply`.
 */
export interface ComposerPrefillDto {
  accountId: number;
  identityId: number | null;
  to: string[];
  cc: string[];
  subject: string;
  body: string;
  inReplyTo: string | null;
  references: string[];
  attachments: ComposeAttachmentDto[];
  source: PrefillSourceDto;
}

/**
 * Name/size metadata for a candidate attachment, returned by `stat_attachments`
 * so the composer can show a chip before the file is ever read. The `path` is
 * handed back unchanged at send time.
 */
export interface AttachmentCandidateDto {
  path: string;
  filename: string;
  sizeBytes: number;
}

/** A saved draft, as returned by `list_drafts`. */
export interface DraftDto {
  id: number;
  accountId: number;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  textBody: string;
  updatedUnix: number;
}

/** Delivery lifecycle state of an outbox item. */
export type OutboxState = "pending" | "sending" | "sent" | "failed";

/** One queued or delivered message in the outbox. */
export interface OutboxItemDto {
  id: number;
  accountId: number;
  subject: string;
  toSummary: string;
  state: OutboxState;
  attempts: number;
  lastError: string | null;
  /** Identity the message will be sent under, if any. */
  identityId?: number | null;
  /** Unix second a scheduled ("send later") message is withheld until. */
  scheduledUnix?: number | null;
  updatedUnix: number;
}

/**
 * Outcome of a send, retry, or schedule attempt. `sent` is true only when the
 * message left the machine; otherwise it sits in the outbox and `error`
 * explains why — unless `scheduledAtUnix` is set, meaning it was queued for
 * later (no error).
 */
export interface SendOutcomeDto {
  outboxId: number;
  sent: boolean;
  error: string | null;
  /** When set, the message was queued to send at this Unix second. */
  scheduledAtUnix?: number | null;
  /**
   * True when the message was queued because the app is working offline; it
   * stays in the outbox and is sent automatically on returning online.
   */
  queuedOffline?: boolean;
}
