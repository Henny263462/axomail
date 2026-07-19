<script setup lang="ts">
/**
 * The message-list region: a persistent search field above the list content.
 *
 * A non-empty query switches the pane into results mode (backed by
 * `useSearch`); clearing it or pressing Escape restores the underlying list —
 * a single mailbox (`view="mailbox"`) or the unified inbox (`view="unified"`).
 * Selection emits a plain message id in every mode, so the host opens the
 * detail exactly as it does for a normal mailbox.
 */
import { computed, ref, watch } from "vue";

import type { BulkAction } from "../../composables/useMailboxActions";
import { useSearch } from "../../composables/useSearch";
import type {
  AccountSummary,
  MessageListItem,
  UnifiedItemDto,
} from "../../types/mail";
import FolderPropertiesDialog from "../offline/FolderPropertiesDialog.vue";
import MessageListPane from "./MessageListPane.vue";
import SearchBox from "./SearchBox.vue";
import SearchResultsPane from "./SearchResultsPane.vue";
import UnifiedListPane from "./UnifiedListPane.vue";

const props = withDefaults(
  defineProps<{
    view: "mailbox" | "unified";
    messages: MessageListItem[];
    unifiedItems: UnifiedItemDto[];
    accounts: AccountSummary[];
    selectedMessageId: number | null;
    /** Identity of the shown mailbox; switching it resets an active search. */
    selectedMailboxId: number | null;
    /** Whether conversations are grouped into threads (persisted preference). */
    threaded?: boolean;
    /** Whether a next page can be loaded for the current list (infinite scroll). */
    hasMore?: boolean;
    /** Whether a next-page fetch is in flight. */
    loadingMore?: boolean;
    /** Bumped by the host to clear the selection after a bulk relocation. */
    selectionResetKey?: number;
  }>(),
  { threaded: false, hasMore: false, loadingMore: false, selectionResetKey: 0 },
);

const emit = defineEmits<{
  select: [messageId: number];
  toggleFlag: [messageId: number];
  markAllRead: [];
  loadMore: [];
  selectionChange: [messageIds: number[]];
  bulkAction: [action: BulkAction, ids: number[]];
  "update:threaded": [value: boolean];
}>();

const { query, results, active, searching, clear } = useSearch();

// A stale result list must never hide a freshly selected mailbox or the
// unified inbox: any change of the shown list resets the search.
watch(
  () => [props.view, props.selectedMailboxId],
  () => {
    clear();
    // Never leave the properties dialog open over a different folder.
    showFolderProps.value = false;
  },
);

// The folder-properties (offline availability) dialog for the shown mailbox.
// Opened from this region's own affordance, deliberately separate from the
// sidebar's folder context menu.
const showFolderProps = ref(false);

/** Whether a single real mailbox is shown (properties apply only then). */
const showFolderButton = computed(
  () => props.view === "mailbox" && props.selectedMailboxId !== null,
);

/** Display name of the shown mailbox, resolved from the account tree. */
const selectedMailboxName = computed(() => {
  for (const account of props.accounts) {
    const mailbox = account.mailboxes.find(
      (m) => m.id === props.selectedMailboxId,
    );
    if (mailbox !== undefined) return mailbox.name;
  }
  return "";
});
</script>

<template>
  <div class="region">
    <div class="toolbar">
      <SearchBox
        v-model="query"
        @escape="clear"
      />
      <button
        v-if="showFolderButton"
        type="button"
        class="folder-props"
        aria-label="Folder offline availability"
        title="Offline availability"
        @click="showFolderProps = true"
      >
        Offline…
      </button>
    </div>
    <SearchResultsPane
      v-if="active"
      :results="results"
      :searching="searching"
      :selected-message-id="selectedMessageId"
      @select="emit('select', $event)"
    />
    <UnifiedListPane
      v-else-if="view === 'unified'"
      :items="unifiedItems"
      :accounts="accounts"
      :selected-message-id="selectedMessageId"
      :threaded="threaded"
      :has-more="hasMore"
      :loading-more="loadingMore"
      :selection-reset-key="selectionResetKey"
      @select="emit('select', $event)"
      @toggle-flag="emit('toggleFlag', $event)"
      @load-more="emit('loadMore')"
      @selection-change="emit('selectionChange', $event)"
      @bulk-action="(action, ids) => emit('bulkAction', action, ids)"
      @update:threaded="emit('update:threaded', $event)"
    />
    <MessageListPane
      v-else
      :messages="messages"
      :selected-message-id="selectedMessageId"
      :threaded="threaded"
      :has-more="hasMore"
      :loading-more="loadingMore"
      :selection-reset-key="selectionResetKey"
      @select="emit('select', $event)"
      @toggle-flag="emit('toggleFlag', $event)"
      @mark-all-read="emit('markAllRead')"
      @load-more="emit('loadMore')"
      @selection-change="emit('selectionChange', $event)"
      @bulk-action="(action, ids) => emit('bulkAction', action, ids)"
      @update:threaded="emit('update:threaded', $event)"
    />
    <Teleport to="body">
      <FolderPropertiesDialog
        v-if="showFolderProps && selectedMailboxId !== null"
        :mailbox-id="selectedMailboxId"
        :mailbox-name="selectedMailboxName"
        @close="showFolderProps = false"
      />
    </Teleport>
  </div>
</template>

<style scoped>
.region {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.region > :last-child {
  flex: 1;
  min-height: 0;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex: none;
}

.toolbar > :first-child {
  flex: 1;
  min-width: 0;
}

.folder-props {
  flex: none;
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  padding: var(--space-1) var(--space-2);
  margin-right: var(--space-2);
  border-radius: var(--radius-sm);
  transition: background var(--transition-fast);
}

.folder-props:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}
</style>
