/**
 * Typed wrappers over the read-receipt (MDN) Tauri commands.
 *
 * Each call resolves to its DTO in the desktop shell, or to `null` in the
 * browser preview where the IPC bridge is absent (ADR-0005).
 */
import { safeInvoke } from "../lib/tauri";
import type { MdnOutcomeDto, ReceiptSettings } from "../types/receipts";

/** Reads the read-receipt honour settings. `null` in the browser preview. */
export function getReceiptSettings(): Promise<ReceiptSettings | null> {
  return safeInvoke<ReceiptSettings>("get_receipt_settings");
}

/** Persists the read-receipt honour settings. `null` in the browser preview. */
export function setReceiptSettings(
  settings: ReceiptSettings,
): Promise<null> {
  return safeInvoke<null>("set_receipt_settings", { settings });
}

/**
 * Sends a read receipt for a message that requested one. The backend
 * authoritatively re-decides the policy, so this is safe to call for any
 * receipt-requesting message. Resolves to `null` in the browser preview.
 */
export function sendMdn(messageId: number): Promise<MdnOutcomeDto | null> {
  return safeInvoke<MdnOutcomeDto>("send_mdn", { messageId });
}
