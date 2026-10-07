/**
 * Typed wrappers over the reply/forward Tauri commands.
 *
 * `prepareReply`/`prepareReplyAll`/`prepareForward` build the composer prefill
 * for a source message; `markReplySource` records that its reply/forward was
 * sent (setting `\Answered` or the `$Forwarded` keyword). Each resolves to
 * `null` in the browser preview where the IPC bridge is absent (ADR-0005).
 */
import { safeInvoke } from "../lib/tauri";
import type { ForwardStyle } from "../types/compose";
import type { ComposerPrefillDto, ReplyMode, SourceMark } from "../types/mail";

/** Builds the composer prefill for a source message in the requested mode. */
function prepare(
  messageId: number,
  mode: ReplyMode,
): Promise<ComposerPrefillDto | null> {
  return safeInvoke<ComposerPrefillDto>("prepare_reply", { messageId, mode });
}

/** Prefill for a reply to the sender only. */
export function prepareReply(
  messageId: number,
): Promise<ComposerPrefillDto | null> {
  return prepare(messageId, "reply");
}

/** Prefill for a reply to the sender and every other recipient. */
export function prepareReplyAll(
  messageId: number,
): Promise<ComposerPrefillDto | null> {
  return prepare(messageId, "replyAll");
}

/** Prefill for a forward to new recipients. */
export function prepareForward(
  messageId: number,
  style?: ForwardStyle,
): Promise<ComposerPrefillDto | null> {
  if (style === "inline") return prepare(messageId, "forwardInline");
  if (style === "attachment") return prepare(messageId, "forwardAttachment");
  return prepare(messageId, "forward");
}

/**
 * Marks a source message after its reply/forward has been sent: `answered`
 * sets `\Answered`, `forwarded` adds the `$Forwarded` keyword. Resolves to
 * `null` in the browser preview.
 */
export function markReplySource(
  messageId: number,
  mark: SourceMark,
): Promise<null> {
  return safeInvoke<null>("mark_reply_source", { messageId, mark });
}
