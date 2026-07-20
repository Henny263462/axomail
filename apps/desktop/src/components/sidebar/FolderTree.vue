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
import { useI18n } from "vue-i18n";

import {
  compactStorage,
  createFolder,
  deleteFolder,
  emptyTrash,
  refreshFolders,
  renameFolder,
  setFolderSubscribed,
} from "../../api/folders";
import { useFolderFavorites } from "../../composables/useFolderFavorites";
import { useFolderImportExport } from "../../composables/useFolderImportExport";
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

const { t } = useI18n({ useScope: "global" });

/** Ids the user explicitly collapsed; everything else is expanded. */
const collapsedIds = reactive(new Set<number>());
const tree = computed(() => buildFolderTree(props.account.mailboxes));

// Favorite-folder marking (the "Add/Remove Favorites" context-menu action).
const { isFavorite, toggleFavorite } = useFolderFavorites();

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
function menuToggleFavorite(): void {
  const folder = menuFolder.value;
  closeMenu();
  if (folder !== null) toggleFavorite(folder.id);
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

// mbox export / import (dialog-driven; own status message).
const portability = useFolderImportExport(() => emit("changed"));
function menuExport(): void {
  const folder = menuFolder.value;
  closeMenu();
  if (folder !== null) void portability.exportFolder(folder);
}
function menuImport(): void {
  const folder = menuFolder.value;
  closeMenu();
  if (folder !== null) void portability.importFolder(folder);
}

const dialogTitle = computed(() => {
  const state = dialog.value;
  if (state === null) return "";
  if (state.kind === "create") {
    return state.parentLeaf === null
      ? t("sidebar.folderTree.newFolder")
      : t("sidebar.folderTree.newSubfolder", { parent: state.parentLeaf });
  }
  if (state.kind === "rename") return t("sidebar.folderTree.renameFolder");
  return state.kind === "emptyTrash"
    ? t("sidebar.folderTree.emptyTrashTitle")
    : t("sidebar.folderTree.deleteFolderTitle");
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
  dialog.value?.kind === "emptyTrash"
    ? t("sidebar.folderTree.emptyTrashTitle")
    : t("common.delete"),
);
/**
 * The word to retype. Empty Trash is irreversible, so it always demands one
 * ("empty"); a folder delete demands the folder name only when it holds mail.
 */
const confirmWord = computed(() => {
  if (dialog.value?.kind === "emptyTrash")
    return t("sidebar.folderTree.confirmWordEmpty");
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
      ? t("sidebar.folderTree.emptyTrashBody", { count })
      : t("sidebar.folderTree.trashAlreadyEmpty");
  }
  return count > 0
    ? t("sidebar.folderTree.deleteFolderBody", { count })
    : t("sidebar.folderTree.deleteEmptyFolderBody");
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
        :aria-label="t('sidebar.folderTree.newTopLevelFolder')"
        :title="t('sidebar.folderTree.newFolder')"
        @click="openDialog({ kind: 'create', parentId: null, parentLeaf: null })"
      >
        + {{ t("sidebar.folderTree.folderShort") }}
      </button>
      <button
        type="button"
        class="tool"
        :aria-label="t('sidebar.folderTree.refreshFolderList')"
        :title="t('sidebar.folderTree.refreshFoldersTitle')"
        @click="refreshFolders(account.id)"
      >
        ↻ {{ t("common.refresh") }}
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
      :is-favorite="isFavorite(menuFolder.id)"
      @create="menuCreate"
      @rename="menuRename"
      @toggle-subscribe="menuToggleSubscribe"
      @toggle-favorite="menuToggleFavorite"
      @properties="menuProperties"
      @compact="menuCompact"
      @export-folder="menuExport"
      @import-folder="menuImport"
      @empty-trash="menuEmptyTrash"
      @delete="menuDelete"
      @close="closeMenu"
    />

    <p
      v-if="portability.message.value !== null"
      class="portability-status"
      role="status"
      aria-live="polite"
    >
      {{ portability.message.value }}
    </p>

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
      :label="t('sidebar.folderDialog.folderName')"
      :initial-value="dialogInitial"
      :confirm-text="dialog.kind === 'rename' ? t('sidebar.folderTree.rename') : t('common.create')"
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

.portability-status {
  padding: var(--space-1) var(--space-2);
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  overflow-wrap: anywhere;
}
</style>
