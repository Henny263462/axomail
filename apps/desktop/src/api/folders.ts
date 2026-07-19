/**
 * Typed wrappers over the folder-management Tauri commands.
 *
 * The CRUD calls are server-first in the backend: they resolve only after the
 * server operation succeeds, so a rejected promise means nothing changed
 * locally. Each rejects with the backend's user-facing error string so the
 * dialog can show it (see {@link invokeOrThrow}); `refreshFolders` uses
 * {@link safeInvoke} since a queue failure is not worth interrupting the user.
 * All resolve to `null` in the browser preview where the bridge is absent
 * (ADR-0005).
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type { CompactOutcome, MailboxSummary } from "../types/mail";

/** Creates a folder under `parentId` (or at the root when null). */
export function createFolder(input: {
  accountId: number;
  parentId: number | null;
  name: string;
}): Promise<MailboxSummary | null> {
  return invokeOrThrow<MailboxSummary>("create_folder", { input });
}

/** Renames a folder, keeping it under the same parent. */
export function renameFolder(
  folderId: number,
  name: string,
): Promise<null> {
  return invokeOrThrow<null>("rename_folder", { input: { folderId, name } });
}

/** Deletes a folder on the server and locally. */
export function deleteFolder(folderId: number): Promise<null> {
  return invokeOrThrow<null>("delete_folder", { folderId });
}

/** Subscribes to or unsubscribes from a folder. */
export function setFolderSubscribed(
  folderId: number,
  subscribed: boolean,
): Promise<null> {
  return invokeOrThrow<null>("set_folder_subscribed", {
    input: { folderId, subscribed },
  });
}

/** Enqueues a folder-discovery refresh for an account. */
export function refreshFolders(accountId: number): Promise<null> {
  return safeInvoke<null>("refresh_folders", { accountId });
}

/**
 * Permanently empties a Trash folder (server expunge + local purge). Rejects on
 * a real backend failure so the folder menu can report it. Resolves the number
 * of local messages removed.
 */
export function emptyTrash(folderId: number): Promise<number | null> {
  return invokeOrThrow<number>("empty_trash", { folderId });
}

/** Reclaims orphaned raw `.eml` blobs. Resolves how many were removed. */
export function compactStorage(): Promise<CompactOutcome | null> {
  return invokeOrThrow<CompactOutcome>("compact_storage", {});
}

/** Reads an account's "empty Trash on exit" setting. */
export function getEmptyTrashOnExit(accountId: number): Promise<boolean | null> {
  return safeInvoke<boolean>("get_empty_trash_on_exit", { accountId });
}

/** Sets an account's "empty Trash on exit" setting. */
export function setEmptyTrashOnExit(
  accountId: number,
  enabled: boolean,
): Promise<null> {
  return invokeOrThrow<null>("set_empty_trash_on_exit", { accountId, enabled });
}
