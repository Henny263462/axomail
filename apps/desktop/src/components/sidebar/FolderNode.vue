<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { FolderTreeNode } from "./folderTree";
import type { SidebarView } from "./view";

const props = defineProps<{
  node: FolderTreeNode;
  selectedMailboxId: number | null;
  activeView: SidebarView;
  /** Ids the user explicitly collapsed; folders are expanded by default. */
  collapsedIds: Set<number>;
}>();

const emit = defineEmits<{
  select: [mailboxId: number];
  toggle: [mailboxId: number];
  openMenu: [payload: { folderId: number; x: number; y: number }];
}>();

const { t } = useI18n({ useScope: "global" });

const mailbox = computed(() => props.node.mailbox);
const hasChildren = computed(() => props.node.children.length > 0);
const isExpanded = computed(() => !props.collapsedIds.has(mailbox.value.id));
const isSelected = computed(
  () =>
    props.activeView === "mailbox" &&
    mailbox.value.id === props.selectedMailboxId,
);
const unread = computed(() => mailbox.value.unreadCount);
const total = computed(() => mailbox.value.totalCount ?? 0);
const subscribed = computed(() => mailbox.value.subscribed !== false);
const orphaned = computed(() => mailbox.value.orphaned === true);
const indent = computed(() => `${props.node.depth * 12 + 8}px`);
const countTitle = computed(() =>
  t("sidebar.folderNode.countTitle", {
    total: total.value,
    unread: unread.value,
  }),
);

function openMenuAt(x: number, y: number): void {
  emit("openMenu", { folderId: mailbox.value.id, x, y });
}
</script>

<template>
  <li class="folder-node">
    <div
      class="folder-row"
      :class="{ selected: isSelected, unsubscribed: !subscribed }"
      :style="{ paddingLeft: indent }"
      @contextmenu.prevent="openMenuAt($event.clientX, $event.clientY)"
    >
      <button
        v-if="hasChildren"
        type="button"
        class="twisty"
        :aria-label="isExpanded ? t('sidebar.folderNode.collapse') : t('sidebar.folderNode.expand')"
        :aria-expanded="isExpanded"
        @click.stop="emit('toggle', mailbox.id)"
      >
        {{ isExpanded ? "▾" : "▸" }}
      </button>
      <span
        v-else
        class="twisty-spacer"
        aria-hidden="true"
      />
      <button
        type="button"
        class="folder-button"
        :class="{ selected: isSelected }"
        :aria-current="isSelected ? 'true' : undefined"
        :title="subscribed ? undefined : t('sidebar.folderNode.notSubscribed', { name: node.leaf })"
        @click="emit('select', mailbox.id)"
      >
        <span class="folder-name">{{ node.leaf }}</span>
        <span
          v-if="orphaned"
          class="orphan"
          :title="t('sidebar.folderNode.orphanHint')"
        >⚠</span>
        <span
          v-if="unread > 0"
          class="unread"
          :title="countTitle"
          :aria-label="t('sidebar.unreadCount', { count: unread })"
        >{{ unread }}</span>
      </button>
      <button
        type="button"
        class="folder-menu"
        :aria-label="t('sidebar.folderNode.folderActions', { name: node.leaf })"
        @click.stop="
          openMenuAt(
            ($event.currentTarget as HTMLElement).getBoundingClientRect().right,
            ($event.currentTarget as HTMLElement).getBoundingClientRect().bottom,
          )
        "
      >
        ⋯
      </button>
    </div>
    <ul
      v-if="hasChildren && isExpanded"
      class="folder-children"
    >
      <FolderNode
        v-for="child in node.children"
        :key="child.mailbox.id"
        :node="child"
        :selected-mailbox-id="selectedMailboxId"
        :active-view="activeView"
        :collapsed-ids="collapsedIds"
        @select="emit('select', $event)"
        @toggle="emit('toggle', $event)"
        @open-menu="emit('openMenu', $event)"
      />
    </ul>
  </li>
</template>

<style scoped>
.folder-node {
  list-style: none;
}

.folder-row {
  display: flex;
  align-items: center;
  gap: 2px;
  border-radius: var(--radius-md);
  padding-right: var(--space-1);
}

.folder-row:hover {
  background: var(--color-bg-hover);
}

.folder-row.selected {
  background: var(--color-bg-selected);
}

.folder-row.unsubscribed .folder-name {
  opacity: 0.55;
  font-style: italic;
}

.twisty,
.twisty-spacer {
  flex-shrink: 0;
  width: 16px;
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  line-height: 1;
}

.twisty:hover {
  color: var(--color-text);
}

.folder-button {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex: 1;
  min-width: 0;
  padding: var(--space-1) 0;
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  text-align: left;
}

.folder-button.selected {
  color: var(--color-text);
  font-weight: var(--weight-medium);
}

.folder-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.orphan {
  flex-shrink: 0;
  color: var(--color-warning, #b45309);
  font-size: var(--text-xs);
}

.unread {
  flex-shrink: 0;
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-accent);
}

.folder-menu {
  flex-shrink: 0;
  visibility: hidden;
  padding: 0 var(--space-1);
  font-size: var(--text-sm);
  line-height: 1;
  color: var(--color-text-secondary);
  border-radius: var(--radius-sm);
}

.folder-row:hover .folder-menu,
.folder-row.selected .folder-menu,
/* Reveal on keyboard focus too, so the actions menu (rename / favorite /
   properties / delete) is reachable without a mouse hover or right-click. */
.folder-row:focus-within .folder-menu,
.folder-menu:focus-visible {
  visibility: visible;
}

.folder-menu:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.folder-children {
  margin: 0;
  padding: 0;
}
</style>
