<script setup lang="ts">
/**
 * Nested folder tree for one account: a toolbar (new folder / refresh), the
 * recursive {@link FolderNode} forest, a right-click/⋯ context menu, and the
 * create/rename/delete dialogs.
 *
 * Folder mutations call the `api/folders` wrappers directly (the api layer is
 * the IPC boundary) and, on success, emit `changed` so the parent reloads
 * account summaries. Selection bubbles up as `selectMailbox`.
 */
import { computed, reactive, ref } from "vue";

import {
  compactStorage,
  createFolder,
  deleteFolder,
  emptyTrash,
  refreshFolders,
  renameFolder,
  setFolderSubscribed,
} from "../../api/folders";
import type { AccountSummary, MailboxSummary } from "../../types/mail";
import FolderPropertiesDialog from "../offline/FolderPropertiesDialog.vue";
import FolderContextMenu from "./FolderContextMenu.vue";
import FolderDialog from "./FolderDialog.vue";
import FolderNode from "./FolderNode.vue";
import { buildFolderTree, leafName } from "./folderTree";
import type { SidebarView } from "./view";

const props = defineProps<{
  account: AccountSummary;
  selectedMailboxId: number | null;
  activeView: SidebarView;
}>();

const emit = defineEmits<{
  selectMailbox: [mailboxId: number];
  changed: [];
}>();

/** Ids the user explicitly collapsed; everything else is expanded. */
const collapsedIds = reactive(new Set<number>());
const tree = computed(() => buildFolderTree(props.account.mailboxes));

const menu = ref<{ folderId: number; x: number; y: number } | null>(null);
// The control that opened the menu; focus returns to it on close (role=menu).
let menuTrigger: HTMLElement | null = null;
const menuFolder = computed<MailboxSummary | null>(() => {
  if (menu.value === null) return null;
  return (
    props.account.mailboxes.find((m) => m.id === menu.value?.folderId) ?? null
  );
});

type DialogState =
  | { kind: "create"; parentId: number | null; parentLeaf: string | null }
  | { kind: "rename"; folder: MailboxSummary }
  | { kind: "delete"; folder: MailboxSummary }
  | { kind: "emptyTrash"; folder: MailboxSummary };
const dialog = ref<DialogState | null>(null);
const busy = ref(false);
const errorMessage = ref("");

/** Folder whose offline-availability properties dialog is open, if any. */
const propertiesFolder = ref<MailboxSummary | null>(null);

function toggle(id: number): void {
  if (collapsedIds.has(id)) collapsedIds.delete(id);
  else collapsedIds.add(id);
}

function openMenu(payload: { folderId: number; x: number; y: number }): void {
  // Remember the opener (the ⋯ button, or the focused row on right-click) so
  // focus can return to it when the menu closes.
  menuTrigger = (document.activeElement as HTMLElement | null) ?? null;
  menu.value = payload;
}
function closeMenu(): void {
  menu.value = null;
  const trigger = menuTrigger;
  menuTrigger = null;
  trigger?.focus();
}

function openDialog(state: DialogState): void {
  errorMessage.value = "";
  dialog.value = state;
}
function closeDialog(): void {
  dialog.value = null;
  busy.value = false;
  errorMessage.value = "";
}

function menuCreate(): void {
  const folder = menuFolder.value;
  closeMenu();
  if (folder !== null) {
    openDialog({ kind: "create", parentId: folder.id, parentLeaf: leafName(folder) });
  }
}
function menuRename(): void {
  const folder = menuFolder.value;
  closeMenu();
  if (folder !== null) openDialog({ kind: "rename", folder });
}
function menuDelete(): void {
  const folder = menuFolder.value;
  closeMenu();
  if (folder !== null) openDialog({ kind: "delete", folder });
}
async function menuToggleSubscribe(): Promise<void> {
  const folder = menuFolder.value;
  closeMenu();
  if (folder === null) return;
  await run(() => setFolderSubscribed(folder.id, folder.subscribed === false));
}
function menuProperties(): void {
  const folder = menuFolder.value;
  closeMenu();
  if (folder !== null) propertiesFolder.value = folder;
}
function menuEmptyTrash(): void {
  const folder = menuFolder.value;
  closeMenu();
  if (folder !== null) openDialog({ kind: "emptyTrash", folder });
}
async function menuCompact(): Promise<void> {
  closeMenu();
  await run(() => compactStorage());
}

const dialogTitle = computed(() => {
  const state = dialog.value;
  if (state === null) return "";
  if (state.kind === "create") {
    return state.parentLeaf === null
      ? "New folder"
      : `New subfolder in ${state.parentLeaf}`;
  }
  if (state.kind === "rename") return "Rename folder";
  return state.kind === "emptyTrash" ? "Empty Trash" : "Delete folder";
});
const dialogInitial = computed(() =>
  dialog.value?.kind === "rename" ? leafName(dialog.value.folder) : "",
);
/** The folder behind a destructive confirm (delete or empty-Trash). */
const confirmFolder = computed(() =>
  dialog.value?.kind === "delete" || dialog.value?.kind === "emptyTrash"
    ? dialog.value.folder
    : null,
);
/** Confirm button label for the destructive dialog. */
const confirmText = computed(() =>
  dialog.value?.kind === "emptyTrash" ? "Empty Trash" : "Delete",
);
/**
 * The word to retype. Empty Trash is irreversible, so it always demands one
 * ("empty"); a folder delete demands the folder name only when it holds mail.
 */
const confirmWord = computed(() => {
  if (dialog.value?.kind === "emptyTrash") return "empty";
  const folder = confirmFolder.value;
  if (folder === null || (folder.totalCount ?? 0) === 0) return undefined;
  return leafName(folder);
});
const confirmBody = computed(() => {
  const folder = confirmFolder.value;
  if (folder === null) return undefined;
  const count = folder.totalCount ?? 0;
  if (dialog.value?.kind === "emptyTrash") {
    return count > 0
      ? `This permanently deletes all ${count} message(s) in Trash from the server and this computer. This cannot be undone.`
      : "Trash is already empty.";
  }
  return count > 0
    ? `This folder holds ${count} message(s). Deleting it on the server also removes them. This cannot be undone.`
    : "This deletes the empty folder on the server.";
});

async function onDialogSubmit(value: string): Promise<void> {
  const state = dialog.value;
  if (state === null) return;
  await run(() => {
    if (state.kind === "create") {
      return createFolder({
        accountId: props.account.id,
        parentId: state.parentId,
        name: value,
      });
    }
    if (state.kind === "rename") return renameFolder(state.folder.id, value);
    if (state.kind === "emptyTrash") return emptyTrash(state.folder.id);
    return deleteFolder(state.folder.id);
  }, true);
}

async function run(op: () => Promise<unknown>, fromDialog = false): Promise<void> {
  busy.value = true;
  errorMessage.value = "";
  try {
    await op();
    if (fromDialog) closeDialog();
    emit("changed");
  } catch (error) {
    errorMessage.value =
      error instanceof Error ? error.message : String(error);
    if (!fromDialog) closeMenu();
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="folder-tree">
    <div class="toolbar">
      <button
        type="button"
        class="tool"
        aria-label="New top-level folder"
        title="New folder"
        @click="openDialog({ kind: 'create', parentId: null, parentLeaf: null })"
      >
        + Folder
      </button>
      <button
        type="button"
        class="tool"
        aria-label="Refresh folder list"
        title="Refresh folders from the server"
        @click="refreshFolders(account.id)"
      >
        ↻ Refresh
      </button>
    </div>

    <ul class="roots">
      <FolderNode
        v-for="node in tree"
        :key="node.mailbox.id"
        :node="node"
        :selected-mailbox-id="selectedMailboxId"
        :active-view="activeView"
        :collapsed-ids="collapsedIds"
        @select="emit('selectMailbox', $event)"
        @toggle="toggle"
        @open-menu="openMenu"
      />
    </ul>

    <FolderContextMenu
      v-if="menu !== null && menuFolder !== null"
      :folder="menuFolder"
      :x="menu.x"
      :y="menu.y"
      @create="menuCreate"
      @rename="menuRename"
      @toggle-subscribe="menuToggleSubscribe"
      @properties="menuProperties"
      @compact="menuCompact"
      @empty-trash="menuEmptyTrash"
      @delete="menuDelete"
      @close="closeMenu"
    />

    <Teleport to="body">
      <FolderPropertiesDialog
        v-if="propertiesFolder !== null"
        :mailbox-id="propertiesFolder.id"
        :mailbox-name="propertiesFolder.name"
        @close="propertiesFolder = null"
      />
    </Teleport>

    <FolderDialog
      v-if="dialog !== null && (dialog.kind === 'create' || dialog.kind === 'rename')"
      :mode="'input'"
      :title="dialogTitle"
      label="Folder name"
      :initial-value="dialogInitial"
      :confirm-text="dialog.kind === 'rename' ? 'Rename' : 'Create'"
      :busy="busy"
      :error-message="errorMessage"
      @submit="onDialogSubmit"
      @cancel="closeDialog"
    />
    <FolderDialog
      v-else-if="dialog !== null"
      :mode="'delete'"
      :title="dialogTitle"
      :body="confirmBody"
      :confirm-text="confirmText"
      :confirm-word="confirmWord"
      :busy="busy"
      :error-message="errorMessage"
      @submit="onDialogSubmit"
      @cancel="closeDialog"
    />
  </div>
</template>

<style scoped>
.folder-tree {
  display: flex;
  flex-direction: column;
}

.toolbar {
  display: flex;
  gap: var(--space-1);
  padding: 0 var(--space-1) var(--space-1);
}

.tool {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  padding: 2px var(--space-1);
  border-radius: var(--radius-sm);
  transition: background var(--transition-fast);
}

.tool:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.roots {
  margin: 0;
  padding: 0;
  list-style: none;
}
</style>
