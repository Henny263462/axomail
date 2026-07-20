/**
 * Folder mbox export / import flow for the sidebar context menu (Track 4.6).
 *
 * Each action picks a path through the native dialog, calls the mbox api, and
 * reports a transient status (or error) message. A successful import invokes
 * `onChanged` so the caller reloads folder summaries (the target folder's
 * message count changed). In the browser preview the api calls resolve to
 * `null` and the flow is a silent no-op (ADR-0005).
 */
import { ref, type Ref } from "vue";

import { exportFolderMbox, importMbox, pickMboxImportPath, pickMboxSavePath } from "../api/mbox";
import { leafName } from "../components/sidebar/folderTree";
import type { MailboxSummary } from "../types/mail";

export interface FolderImportExport {
  /** True while a dialog-driven export/import is in flight. */
  busy: Ref<boolean>;
  /** Latest status or error message, or `null` when idle. */
  message: Ref<string | null>;
  /** Prompts for a destination and exports `folder` to an mbox file. */
  exportFolder(folder: MailboxSummary): Promise<void>;
  /** Prompts for a source and imports it into `folder`. */
  importFolder(folder: MailboxSummary): Promise<void>;
}

export function useFolderImportExport(onChanged: () => void): FolderImportExport {
  const busy = ref(false);
  const message = ref<string | null>(null);

  async function exportFolder(folder: MailboxSummary): Promise<void> {
    const dest = await pickMboxSavePath(`${leafName(folder)}.mbox`);
    if (dest === null) return;
    await run(async () => {
      const result = await exportFolderMbox(folder.id, dest);
      if (result === null) return null;
      const skipped = result.skipped > 0 ? `, ${result.skipped} skipped` : "";
      return `Exported ${result.exported} message(s)${skipped}.`;
    });
  }

  async function importFolder(folder: MailboxSummary): Promise<void> {
    const src = await pickMboxImportPath();
    if (src === null) return;
    await run(async () => {
      const result = await importMbox(folder.id, src);
      if (result === null) return null;
      onChanged();
      return `Imported ${result.imported} message(s) into ${leafName(folder)}.`;
    });
  }

  async function run(op: () => Promise<string | null>): Promise<void> {
    busy.value = true;
    message.value = null;
    try {
      message.value = await op();
    } catch (error) {
      message.value = error instanceof Error ? error.message : String(error);
    } finally {
      busy.value = false;
    }
  }

  return { busy, message, exportFolder, importFolder };
}
