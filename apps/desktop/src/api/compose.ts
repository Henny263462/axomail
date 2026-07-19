/**
 * Typed wrappers over the compose / draft / outbox Tauri commands.
 *
 * Each call resolves to its DTO in the desktop shell, or to `null` in the
 * browser preview where the IPC bridge is absent (ADR-0005). No body text
 * is retained by these wrappers; they only forward it to the backend.
 */
import { safeInvoke } from "../lib/tauri";
import type {
  ComposeInput,
  DraftDto,
  OutboxItemDto,
  SendOutcomeDto,
} from "../types/mail";

/**
 * Submits a message for delivery. The backend always records an outbox
 * entry; `sent` in the outcome reports whether it left the machine.
 * Resolves to `null` in the browser preview.
 */
export function sendMessage(
  input: ComposeInput,
): Promise<SendOutcomeDto | null> {
  return safeInvoke<SendOutcomeDto>("send_message", { input });
}

/**
 * Saves a draft, creating a new one when `draftId` is `null` or updating
 * the identified draft otherwise. Resolves to the draft id, or `null` in
 * the browser preview.
 */
export function saveDraft(
  input: ComposeInput,
  draftId: number | null,
): Promise<number | null> {
  return safeInvoke<number>("save_draft", { input, draftId });
}

/** Lists every saved draft, newest first. Resolves to `null` in preview. */
export function listDrafts(): Promise<DraftDto[] | null> {
  return safeInvoke<DraftDto[]>("list_drafts");
}

/** Permanently removes a draft. Resolves to `null` in the browser preview. */
export function deleteDraft(draftId: number): Promise<null> {
  return safeInvoke<null>("delete_draft", { draftId });
}

/** Lists every outbox item with its delivery state. `null` in preview. */
export function listOutbox(): Promise<OutboxItemDto[] | null> {
  return safeInvoke<OutboxItemDto[]>("list_outbox");
}

/**
 * Re-attempts delivery of a failed outbox item, returning the new outcome.
 * Resolves to `null` in the browser preview.
 */
export function retryOutbox(
  outboxId: number,
): Promise<SendOutcomeDto | null> {
  return safeInvoke<SendOutcomeDto>("retry_outbox", { outboxId });
}

/**
 * Sends a scheduled (or queued) outbox item immediately, clearing any pending
 * "send later" schedule first. Resolves to `null` in the browser preview.
 */
export function sendOutboxNow(
  outboxId: number,
): Promise<SendOutcomeDto | null> {
  return safeInvoke<SendOutcomeDto>("send_outbox_now", { outboxId });
}

/**
 * Cancels a scheduled send, returning its content to a new draft and removing
 * the outbox row. Resolves to the new draft id, or `null` in the browser
 * preview.
 */
export function cancelScheduledSend(
  outboxId: number,
): Promise<number | null> {
  return safeInvoke<number>("cancel_scheduled_send", { outboxId });
}
