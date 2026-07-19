<script setup lang="ts">
/**
 * The unified-inbox list pane: the shared {@link ThreadedMessageList}
 * configured to tag every row with its account's display name.
 */
import type { BulkAction } from "../../composables/useMailboxActions";
import type { AccountSummary, UnifiedItemDto } from "../../types/mail";
import ThreadedMessageList from "./ThreadedMessageList.vue";

withDefaults(
  defineProps<{
    items: UnifiedItemDto[];
    /** Accounts, used to label each row with its account display name. */
    accounts: AccountSummary[];
    selectedMessageId: number | null;
    /** Whether conversations are grouped into threads. */
    threaded?: boolean;
    /** Whether a next page can be loaded (infinite scroll). */
    hasMore?: boolean;
    /** Whether a next-page fetch is in flight. */
    loadingMore?: boolean;
  }>(),
  { threaded: false, hasMore: false, loadingMore: false },
);

const emit = defineEmits<{
  select: [messageId: number];
  toggleFlag: [messageId: number];
  loadMore: [];
  selectionChange: [messageIds: number[]];
  bulkAction: [action: BulkAction, ids: number[]];
  "update:threaded": [value: boolean];
}>();
</script>

<template>
  <ThreadedMessageList
    :messages="items"
    :accounts="accounts"
    :show-account="true"
    :selected-message-id="selectedMessageId"
    :threaded="threaded"
    list-label="Unified inbox"
    empty-detail="Messages from every account's inbox appear here once synchronized."
    :has-more="hasMore"
    :loading-more="loadingMore"
    @select="emit('select', $event)"
    @toggle-flag="emit('toggleFlag', $event)"
    @load-more="emit('loadMore')"
    @selection-change="emit('selectionChange', $event)"
    @bulk-action="(action, ids) => emit('bulkAction', action, ids)"
    @update:threaded="emit('update:threaded', $event)"
  />
</template>
