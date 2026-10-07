<script setup lang="ts">
/**
 * A flat, cross-account list of folders for the sidebar's Favorite and Recent
 * view modes. Each row shows the folder's leaf name (with its account as
 * context) and unread count, and selects the folder on click. An honest empty
 * state is shown when the list is empty.
 */
import { useI18n } from "vue-i18n";

import type { MailboxSummary } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";
import AppIcon from "../common/AppIcon.vue";
import { folderIcon } from "../common/icons";
import { leafName } from "./folderTree";

/** One flattened folder entry: the mailbox plus its owning account's name. */
export interface FlatFolder {
  mailbox: MailboxSummary;
  accountName: string;
}

defineProps<{
  folders: FlatFolder[];
  selectedMailboxId: number | null;
  emptyTitle: string;
  emptyDetail: string;
}>();

const emit = defineEmits<{ selectMailbox: [mailboxId: number] }>();

const { t } = useI18n({ useScope: "global" });
</script>

<template>
  <div class="folder-list">
    <EmptyState
      v-if="folders.length === 0"
      :title="emptyTitle"
      :detail="emptyDetail"
    />
    <ul
      v-else
      class="rows"
    >
      <li
        v-for="entry in folders"
        :key="entry.mailbox.id"
      >
        <button
          type="button"
          class="row"
          :class="{ selected: entry.mailbox.id === selectedMailboxId }"
          :aria-current="entry.mailbox.id === selectedMailboxId ? 'true' : undefined"
          @click="emit('selectMailbox', entry.mailbox.id)"
        >
          <AppIcon
            :name="folderIcon(entry.mailbox.role)"
            class="folder-icon"
          />
          <span class="text">
            <span class="leaf">{{ leafName(entry.mailbox) }}</span>
            <span class="account">{{ entry.accountName }}</span>
          </span>
          <span
            v-if="entry.mailbox.unreadCount > 0"
            class="unread"
            :aria-label="t('sidebar.unreadCount', { count: entry.mailbox.unreadCount })"
          >
            {{ entry.mailbox.unreadCount }}
          </span>
        </button>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.rows {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-md);
  text-align: left;
  color: var(--color-text-secondary);
  transition: background var(--transition-fast);
}

.row:hover {
  background: var(--color-bg-hover);
}

.row.selected {
  background: var(--color-accent-subtle);
  color: var(--color-accent);
  font-weight: var(--weight-medium);
}

.folder-icon {
  color: var(--color-text-tertiary);
}

.row.selected .folder-icon {
  color: var(--color-accent);
}

.text {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.leaf {
  font-size: var(--text-sm);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.account {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.unread {
  flex-shrink: 0;
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-accent);
}

@media (prefers-reduced-motion: reduce) {
  .row {
    transition: none;
  }
}
</style>
