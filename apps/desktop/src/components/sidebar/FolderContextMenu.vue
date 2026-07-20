<script setup lang="ts">
/**
 * The folder right-click menu, extracted from {@link FolderTree} so the tree
 * stays under the file-size gate. Renders a positioned `role="menu"` with a
 * backdrop; the parent owns the folder mutations and re-opens focus on close.
 *
 * Empty Trash is offered only on a `trash`-role folder (the one place
 * server-side removal happens); Compact is offered on any folder.
 */
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import type { MailboxSummary } from "../../types/mail";
import { leafName } from "./folderTree";

withDefaults(
  defineProps<{
    folder: MailboxSummary;
    x: number;
    y: number;
    /** Whether this folder is currently marked as a favorite. */
    isFavorite?: boolean;
  }>(),
  { isFavorite: false },
);

const emit = defineEmits<{
  create: [];
  rename: [];
  toggleSubscribe: [];
  toggleFavorite: [];
  properties: [];
  exportFolder: [];
  importFolder: [];
  delete: [];
  emptyTrash: [];
  compact: [];
  close: [];
}>();

const { t } = useI18n({ useScope: "global" });

const menuEl = ref<HTMLElement | null>(null);

onMounted(() => {
  menuEl.value?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
});

/** role=menu keyboard handling: Escape closes; Up/Down cycle the items. */
function onMenuKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    emit("close");
    return;
  }
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  event.preventDefault();
  const items = Array.from(
    menuEl.value?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [],
  );
  if (items.length === 0) return;
  const current = items.indexOf(document.activeElement as HTMLElement);
  const delta = event.key === "ArrowDown" ? 1 : -1;
  const next = (current + delta + items.length) % items.length;
  items[next]?.focus();
}
</script>

<template>
  <div
    class="menu-backdrop"
    @click="emit('close')"
    @contextmenu.prevent="emit('close')"
  />
  <div
    ref="menuEl"
    class="menu"
    role="menu"
    :aria-label="t('sidebar.folderContext.menuLabel', { name: leafName(folder) })"
    :style="{ left: `${x}px`, top: `${y}px` }"
    @keydown="onMenuKeydown"
  >
    <button
      type="button"
      role="menuitem"
      @click="emit('create')"
    >
      {{ t("sidebar.folderContext.newSubfolder") }}
    </button>
    <button
      type="button"
      role="menuitem"
      @click="emit('rename')"
    >
      {{ t("sidebar.folderContext.rename") }}
    </button>
    <button
      type="button"
      role="menuitem"
      @click="emit('toggleSubscribe')"
    >
      {{ folder.subscribed === false ? t("sidebar.folderContext.subscribe") : t("sidebar.folderContext.unsubscribe") }}
    </button>
    <button
      type="button"
      role="menuitem"
      @click="emit('toggleFavorite')"
    >
      {{ isFavorite ? t("folderView.removeFavorite") : t("folderView.addFavorite") }}
    </button>
    <button
      type="button"
      role="menuitem"
      @click="emit('properties')"
    >
      {{ t("sidebar.folderContext.properties") }}
    </button>
    <button
      type="button"
      role="menuitem"
      @click="emit('compact')"
    >
      {{ t("sidebar.folderContext.compact") }}
    </button>
    <button
      type="button"
      role="menuitem"
      @click="emit('exportFolder')"
    >
      {{ t("sidebar.folderContext.exportFolder") }}
    </button>
    <button
      type="button"
      role="menuitem"
      @click="emit('importFolder')"
    >
      {{ t("sidebar.folderContext.importFolder") }}
    </button>
    <button
      v-if="folder.role === 'trash'"
      type="button"
      role="menuitem"
      class="danger"
      @click="emit('emptyTrash')"
    >
      {{ t("sidebar.folderContext.emptyTrash") }}
    </button>
    <button
      type="button"
      role="menuitem"
      class="danger"
      @click="emit('delete')"
    >
      {{ t("sidebar.folderContext.delete") }}
    </button>
  </div>
</template>

<style scoped>
.menu-backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
}

.menu {
  position: fixed;
  z-index: 101;
  min-width: 160px;
  display: flex;
  flex-direction: column;
  padding: var(--space-1);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  box-shadow: 0 4px 16px rgb(0 0 0 / 18%);
}

.menu button {
  text-align: left;
  font-size: var(--text-sm);
  color: var(--color-text);
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
}

.menu button:hover {
  background: var(--color-bg-hover);
}

.menu button.danger {
  color: var(--color-danger);
}
</style>
