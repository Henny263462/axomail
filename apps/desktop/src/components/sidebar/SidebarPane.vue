<script setup lang="ts">
import { computed } from "vue";

import type { AccountSummary } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";
import FolderTree from "./FolderTree.vue";
import SidebarTagsPane from "./SidebarTagsPane.vue";
import type { SidebarView } from "./view";

const props = withDefaults(
  defineProps<{
    accounts: AccountSummary[];
    selectedMailboxId: number | null;
    /** Active top-level view; drives Drafts/Outbox highlighting. */
    activeView?: SidebarView;
    /** Account ids with a sync job currently running. */
    syncingAccountIds?: number[];
    /** Active tag filter, for highlighting the Tags group. */
    activeTagId?: number | null;
  }>(),
  { activeView: "mailbox", syncingAccountIds: () => [], activeTagId: null },
);

const emit = defineEmits<{
  selectMailbox: [mailboxId: number];
  selectUnified: [];
  addAccount: [];
  compose: [];
  syncAccount: [accountId: number];
  selectDrafts: [];
  selectOutbox: [];
  selectTag: [tagId: number, name: string];
  foldersChanged: [];
}>();

const hasAccounts = computed(() => props.accounts.length > 0);

function isSyncing(accountId: number): boolean {
  return props.syncingAccountIds.includes(accountId);
}
</script>

<template>
  <nav
    class="sidebar"
    aria-label="Accounts and folders"
  >
    <button
      type="button"
      class="add-account"
      @click="emit('addAccount')"
    >
      Add account
    </button>
    <button
      type="button"
      class="compose"
      :disabled="!hasAccounts"
      :title="hasAccounts ? undefined : 'Add an account before composing'"
      @click="emit('compose')"
    >
      Compose
    </button>

    <button
      type="button"
      class="all-inboxes"
      :class="{ selected: activeView === 'unified' }"
      :aria-current="activeView === 'unified' ? 'true' : undefined"
      @click="emit('selectUnified')"
    >
      All inboxes
    </button>

    <EmptyState
      v-if="!hasAccounts"
      title="No accounts"
      detail="Add an account to get started."
    />
    <template v-else>
      <section
        v-for="account in accounts"
        :key="account.id"
        class="account"
        :aria-label="account.displayName"
      >
        <div class="account-header">
          <h2
            class="account-name"
            :title="account.email"
          >
            {{ account.displayName }}
          </h2>
          <span
            v-if="isSyncing(account.id)"
            class="syncing"
            role="status"
          >Syncing…</span>
          <button
            type="button"
            class="sync"
            :aria-label="`Sync ${account.displayName}`"
            title="Sync this account"
            @click="emit('syncAccount', account.id)"
          >
            ↻
          </button>
        </div>
        <FolderTree
          :account="account"
          :selected-mailbox-id="selectedMailboxId"
          :active-view="activeView"
          @select-mailbox="emit('selectMailbox', $event)"
          @changed="emit('foldersChanged')"
        />
      </section>
    </template>

    <!-- Track 2.4 mount point: the Tags filter group. A self-contained child
         component so it does not collide with the folder-tree rewrite. -->
    <SidebarTagsPane
      :active-tag-id="activeTagId"
      @select-tag="(tagId, name) => emit('selectTag', tagId, name)"
    />

    <div class="fixed-views">
      <button
        type="button"
        class="mailbox special"
        :class="{ selected: activeView === 'drafts' }"
        :aria-current="activeView === 'drafts' ? 'true' : undefined"
        @click="emit('selectDrafts')"
      >
        Drafts
      </button>
      <button
        type="button"
        class="mailbox special"
        :class="{ selected: activeView === 'outbox' }"
        :aria-current="activeView === 'outbox' ? 'true' : undefined"
        @click="emit('selectOutbox')"
      >
        Outbox
      </button>
    </div>
  </nav>
</template>

<style scoped>
.sidebar {
  height: 100%;
  overflow-y: auto;
  background: var(--color-bg-subtle);
  border-right: 1px solid var(--color-border);
  padding: var(--space-3);
  display: flex;
  flex-direction: column;
}

.add-account,
.compose {
  display: block;
  width: 100%;
  padding: var(--space-2);
  margin-bottom: var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text);
  background: var(--color-bg);
  transition: background var(--transition-fast);
}

.compose {
  margin-bottom: var(--space-3);
}

.add-account:hover,
.compose:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.compose:disabled {
  opacity: 0.6;
  cursor: default;
}

.account-header {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-2) var(--space-1);
}

.account-name {
  flex: 1;
  min-width: 0;
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.syncing {
  font-size: var(--text-xs);
  color: var(--color-accent);
}

.sync {
  flex-shrink: 0;
  font-size: var(--text-sm);
  line-height: 1;
  color: var(--color-text-secondary);
  padding: 2px var(--space-1);
  border-radius: var(--radius-sm);
  transition: background var(--transition-fast);
}

.sync:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.mailbox,
.all-inboxes {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  transition: background var(--transition-fast);
}

.all-inboxes {
  margin-bottom: var(--space-2);
  font-weight: var(--weight-medium);
  color: var(--color-text);
}

.mailbox:hover,
.all-inboxes:hover {
  background: var(--color-bg-hover);
}

.mailbox.selected,
.all-inboxes.selected {
  background: var(--color-bg-selected);
  color: var(--color-text);
  font-weight: var(--weight-medium);
}

.unread {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-accent);
}

.fixed-views {
  margin-top: auto;
  padding-top: var(--space-3);
  border-top: 1px solid var(--color-border);
  display: flex;
  flex-direction: column;
  gap: 2px;
}
</style>
