/**
 * Typed wrappers for attachment and raw-message export commands (Track 3.6).
 *
 * The user picks a destination through the native file dialog (open/save,
 * ADR-0010); the chosen path is handed to a Rust command that does the disk IO
 * with `std::fs`. The webview never reads or writes files itself. Each call
 * resolves to `null` in the browser preview where the IPC bridge is absent
 * (ADR-0005).
 */
import { safeInvoke } from "../lib/tauri";
import type { SavedAttachmentDto } from "../types/mail";

/** A named extension group offered by the save dialog. */
export interface DialogFilter {
  name: string;
  extensions: string[];
}

/**
 * Opens the OS save-file dialog seeded with `defaultName`, returning the
 * chosen absolute path, or `null` when the user cancels or the bridge is
 * absent. Goes through the granted `dialog:allow-save` permission.
 */
export function pickSaveFilePath(
  defaultName: string,
  filters: DialogFilter[] = [],
): Promise<string | null> {
  return safeInvoke<string>("plugin:dialog|save", {
    options: { defaultPath: defaultName, filters },
  });
}

/**
 * Opens the OS folder picker (for "save all"), returning the chosen directory
 * path, or `null` on cancel / absent bridge. Goes through `dialog:allow-open`.
 */
export function pickDirectoryPath(
  title = "Save all attachments",
): Promise<string | null> {
  return safeInvoke<string>("plugin:dialog|open", {
    options: { directory: true, multiple: false, title },
  });
}

/** Saves one attachment's bytes to `destPath`. Resolves the saved record. */
export function saveAttachment(
  messageId: number,
  attachmentIndex: number,
  destPath: string,
): Promise<SavedAttachmentDto | null> {
  return safeInvoke<SavedAttachmentDto>("save_attachment", {
    messageId,
    attachmentIndex,
    destPath,
  });
}

/** Saves every downloadable attachment into `destDir` (sanitized names). */
export function saveAllAttachments(
  messageId: number,
  destDir: string,
): Promise<SavedAttachmentDto[] | null> {
  return safeInvoke<SavedAttachmentDto[]>("save_all_attachments", {
    messageId,
    destDir,
  });
}

/** Returns the raw `.eml` source text of a message for the view-source pane. */
export function getMessageSource(messageId: number): Promise<string | null> {
  return safeInvoke<string>("get_message_source", { messageId });
}

/** Returns the raw header block of a message for the all-headers toggle. */
export function getMessageHeaders(messageId: number): Promise<string | null> {
  return safeInvoke<string>("get_message_headers", { messageId });
}

/** Saves the whole raw message to `destPath` as a `.eml` file. */
export function saveMessageEml(
  messageId: number,
  destPath: string,
): Promise<number | null> {
  return safeInvoke<number>("save_message_eml", { messageId, destPath });
}
