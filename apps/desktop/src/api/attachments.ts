/**
 * Attachment access, kept behind the api layer so components never call Tauri
 * `invoke` directly (ADR-0010). Two directions live here:
 *
 * - Composer IN: the native open-file dialog and OS file drops yield *paths
 *   only*; {@link statAttachments} turns those into name/size metadata through a
 *   Rust `std::fs` command, and the send command reads the bytes later in Rust.
 * - Reading-pane OUT: the user picks a destination through the native
 *   open/save dialog; the chosen path is handed to a Rust command that does the
 *   disk IO with `std::fs` ({@link saveAttachment} and friends, Track 3.6).
 *
 * The webview never reads or writes files itself. Every call degrades to
 * `null`/no-op in the browser preview where the IPC bridge is absent (ADR-0005).
 */
import { isTauri, safeInvoke } from "../lib/tauri";
import type { AttachmentCandidateDto, SavedAttachmentDto } from "../types/mail";

/** A named extension group offered by the save dialog. */
export interface DialogFilter {
  name: string;
  extensions: string[];
}

/**
 * Opens the native multi-select open-file dialog and resolves to the chosen
 * absolute paths, or `null` when the user cancels or the desktop bridge is
 * unavailable.
 */
export async function pickAttachments(): Promise<string[] | null> {
  const chosen = await safeInvoke<string | string[] | null>(
    "plugin:dialog|open",
    { options: { multiple: true, directory: false, title: "Attach files" } },
  );
  if (chosen === null || chosen === undefined) return null;
  return Array.isArray(chosen) ? chosen : [chosen];
}

/**
 * Returns name/size metadata for the given attachment paths, or `null` in the
 * browser preview. Rejects a selection whose combined size exceeds the backend
 * cap (surfaced as an error the caller can show).
 */
export function statAttachments(
  paths: string[],
): Promise<AttachmentCandidateDto[] | null> {
  return safeInvoke<AttachmentCandidateDto[]>("stat_attachments", { paths });
}

/**
 * Subscribes to OS file drops onto the app window, invoking `handler` with the
 * dropped paths. Returns an unsubscribe function. A no-op (returning an empty
 * disposer) outside the desktop shell, so the browser preview and tests are
 * unaffected. Drag-drop is a best-effort convenience; the dialog picker is the
 * reliable path.
 */
export function onFilesDropped(handler: (paths: string[]) => void): () => void {
  if (!isTauri()) return () => {};
  let unlisten: (() => void) | null = null;
  let disposed = false;
  void (async () => {
    try {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      const stop = await getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === "drop") handler(event.payload.paths);
      });
      if (disposed) stop();
      else unlisten = stop;
    } catch (error) {
      console.error("attachment drag-drop subscription failed", error);
    }
  })();
  return () => {
    disposed = true;
    unlisten?.();
  };
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
