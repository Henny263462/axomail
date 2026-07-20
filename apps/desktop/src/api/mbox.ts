/**
 * Typed wrappers over the mbox export/import commands (Track 4.6).
 *
 * Path picking goes through the granted dialog permissions (ADR-0010); the
 * chosen path is handed to a Rust command that reads/writes the file with
 * `std::fs`. The webview never touches the filesystem itself. Every call
 * degrades to `null` in the browser preview where the bridge is absent
 * (ADR-0005).
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type { MboxExportDto, MboxImportDto } from "../types/mail";

/**
 * Opens the OS save-file dialog seeded with `defaultName` for an mbox export,
 * returning the chosen path or `null` on cancel / absent bridge.
 */
export function pickMboxSavePath(defaultName: string): Promise<string | null> {
  return safeInvoke<string>("plugin:dialog|save", {
    options: {
      defaultPath: defaultName,
      filters: [{ name: "Mailbox (mbox)", extensions: ["mbox"] }],
    },
  });
}

/**
 * Opens the OS open-file dialog for an mbox / `.eml` import, returning the
 * chosen path or `null` on cancel / absent bridge. No extension filter is set
 * so extensionless mbox files (as many mail clients produce) stay selectable.
 */
export async function pickMboxImportPath(): Promise<string | null> {
  const chosen = await safeInvoke<string | string[]>("plugin:dialog|open", {
    options: { directory: false, multiple: false, title: "Import mbox or .eml file" },
  });
  if (chosen === null) return null;
  return Array.isArray(chosen) ? (chosen[0] ?? null) : chosen;
}

/** Exports a folder's messages to the mbox file at `destPath`. */
export function exportFolderMbox(
  mailboxId: number,
  destPath: string,
): Promise<MboxExportDto | null> {
  return invokeOrThrow<MboxExportDto>("export_folder_mbox", { mailboxId, destPath });
}

/** Imports an mbox / `.eml` file into a folder as local messages. */
export function importMbox(
  mailboxId: number,
  srcPath: string,
): Promise<MboxImportDto | null> {
  return invokeOrThrow<MboxImportDto>("import_mbox", { mailboxId, srcPath });
}
