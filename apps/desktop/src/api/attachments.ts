/**
 * Composer attachment access, kept behind the api layer so components never
 * call Tauri `invoke` directly (ADR-0010).
 *
 * The native open-file dialog and OS file drops yield *paths only*; the webview
 * has no filesystem scope. {@link statAttachments} turns those paths into
 * name/size metadata through a Rust `std::fs` command, and the send command
 * reads the bytes later — again in Rust. Every call degrades to `null`/no-op in
 * the browser preview where the IPC bridge is absent (ADR-0005).
 */
import { isTauri, safeInvoke } from "../lib/tauri";
import type { AttachmentCandidateDto } from "../types/mail";

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
