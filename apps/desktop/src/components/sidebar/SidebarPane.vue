<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useFolderFavorites } from "../../composables/useFolderFavorites";
import { useFolderView } from "../../composables/useFolderView";
import { useRecentFolders } from "../../composables/useRecentFolders";
import type { AccountSummary } from "../../types/mail";
import type { SavedSearchDto } from "../../types/search";
import EmptyState from "../common/EmptyState.vue";
import FolderTree from "./FolderTree.vue";
import SidebarFolderList, { type FlatFolder } from "./SidebarFolderList.vue";
import SidebarSavedSearchesPane from "./SidebarSavedSearchesPane.vue";
import SidebarTagsPane from "./SidebarTagsPane.vue";
import SidebarViewSwitcher from "./SidebarViewSwitcher.vue";
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
    /** Bumped to make the Tags group reload after a tag/sync change. */
    tagsRefreshKey?: number;
    /** Active saved search, for highlighting the Saved searches group. */
    activeSavedSearchId?: number | null;
    /** Bumped to make the Saved searches group reload after a change. */
    savedSearchRefreshKey?: number;
  }>(),
  {
    activeView: "mailbox",
    syncingAccountIds: () => [],
    activeTagId: null,
    tagsRefreshKey: 0,
    activeSavedSearchId: null,
    savedSearchRefreshKey: 0,
  },
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
  selectSavedSearch: [saved: SavedSearchDto];
  editSavedSearch: [saved: SavedSearchDto];
  savedSearchesChanged: [];
}>();

const { t } = useI18n({ useScope: "global" });
const { mode: folderMode, setMode: setFolderMode } = useFolderView();
const { favoriteIds } = useFolderFavorites();
const { recentIds, recordFolder } = useRecentFolders();

const hasAccounts = computed(() => props.accounts.length > 0);

/** Every account's mailboxes, indexed by id, for the flat favorite/recent lists. */
const folderIndex = computed(() => {
  const map = new Map<number, FlatFolder>();
  for (const account of props.accounts) {
    for (const mailbox of account.mailboxes) {
      map.set(mailbox.id, { mailbox, accountName: account.displayName });
    }
  }
  return map;
});

/** Resolves an ordered id list to live folder entries, dropping stale ids. */
function resolveFolders(ids: number[]): FlatFolder[] {
  return ids
    .map((id) => folderIndex.value.get(id))
    .filter((entry): entry is FlatFolder => entry !== undefined);
}

const favoriteFolders = computed(() => resolveFolders(favoriteIds.value));
const recentFolders = computed(() => resolveFolders(recentIds.value));

function isSyncing(accountId: number): boolean {
  return props.syncingAccountIds.includes(accountId);
}

/** Records the opened folder as recent, then bubbles the selection up. */
function onSelectMailbox(mailboxId: number): void {
  recordFolder(mailboxId);
  emit("selectMailbox", mailboxId);
}
</script>

<template>
  <nav
    class="sidebar"
    :aria-label="t('sidebar.ariaLabel')"
  >
    <button
      type="button"
      class="add-account"
      @click="emit('addAccount')"
    >
      {{ t("sidebar.addAccount") }}
    </button>
    <button
      type="button"
      class="compose"
      :disabled="!hasAccounts"
      :title="hasAccounts ? undefined : t('sidebar.composeDisabledHint')"
      @click="emit('compose')"
    >
      {{ t("sidebar.compose") }}
    </button>

    <SidebarViewSwitcher
      :mode="folderMode"
      @select="setFolderMode"
    />

    <button
      v-if="folderMode === 'all' || folderMode === 'unified'"
      type="button"
      class="all-inboxes"
      :class="{ selected: activeView === 'unified' }"
      :aria-current="activeView === 'unified' ? 'true' : undefined"
      @click="emit('selectUnified')"
    >
      {{ t("sidebar.allInboxes") }}
    </button>

    <!-- All folders: the full per-account trees (the default view). -->
    <template v-if="folderMode === 'all'">
      <EmptyState
        v-if="!hasAccounts"
        :title="t('sidebar.noAccounts')"
        :detail="t('sidebar.noAccountsHint')"
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
            >{{ t("sidebar.syncing") }}</span>
            <button
              type="button"
              class="sync"
              :aria-label="t('sidebar.syncAccount', { name: account.displayName })"
              :title="t('sidebar.syncThisAccount')"
              @click="emit('syncAccount', account.id)"
            >
              ↻
            </button>
          </div>
          <FolderTree
            :account="account"
            :selected-mailbox-id="selectedMailboxId"
            :active-view="activeView"
            @select-mailbox="onSelectMailbox"
            @changed="emit('foldersChanged')"
          />
        </section>
      </template>
    </template>

    <!-- Favorite folders: the folders the user marked as favorites. -->
    <SidebarFolderList
      v-else-if="folderMode === 'favorite'"
      :folders="favoriteFolders"
      :selected-mailbox-id="selectedMailboxId"
      :empty-title="t('folderView.favoritesEmpty')"
      :empty-detail="t('folderView.favoritesEmptyHint')"
      @select-mailbox="onSelectMailbox"
    />

    <!-- Recent folders: recently opened folders, newest first. -->
    <SidebarFolderList
      v-else-if="folderMode === 'recent'"
      :folders="recentFolders"
      :selected-mailbox-id="selectedMailboxId"
      :empty-title="t('folderView.recentEmpty')"
      :empty-detail="t('folderView.recentEmpty')"
      @select-mailbox="onSelectMailbox"
    />

    <!-- Track 2.4 mount point: the Tags filter group. Shown in the full "all"
         view and as its own focused "tags" mode. -->
    <SidebarTagsPane
      v-if="folderMode === 'all' || folderMode === 'tags'"
      :active-tag-id="activeTagId"
      :refresh-key="tagsRefreshKey"
      @select-tag="(tagId, name) => emit('selectTag', tagId, name)"
    />

    <!-- Track 5.2 mount point: the Saved searches (virtual folders) group. A
         self-contained child component so it never collides with the folder
         tree or the Tags group. -->
    <SidebarSavedSearchesPane
      :active-saved-search-id="activeSavedSearchId"
      :refresh-key="savedSearchRefreshKey"
      @select="emit('selectSavedSearch', $event)"
      @edit="emit('editSavedSearch', $event)"
      @changed="emit('savedSearchesChanged')"
    />

    <div class="fixed-views">
      <button
        type="button"
        class="mailbox special"
        :class="{ selected: activeView === 'drafts' }"
        :aria-current="activeView === 'drafts' ? 'true' : undefined"
        @click="emit('selectDrafts')"
      >
        {{ t("sidebar.drafts") }}
      </button>
      <button
        type="button"
        class="mailbox special"
        :class="{ selected: activeView === 'outbox' }"
        :aria-current="activeView === 'outbox' ? 'true' : undefined"
        @click="emit('selectOutbox')"
      >
        {{ t("sidebar.outbox") }}
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
