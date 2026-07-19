<script setup lang="ts">
/**
 * The single-mailbox / tag-filter list pane: a thin configuration of the
 * shared {@link ThreadedMessageList} (mailbox labelling, a "Mark all read"
 * header, no per-row account tag).
 */
import type { BulkAction } from "../../composables/useMailboxActions";
import type { MessageListItem as MessageListItemModel } from "../../types/mail";
import ThreadedMessageList from "./ThreadedMessageList.vue";

withDefaults(
  defineProps<{
    messages: MessageListItemModel[];
    selectedMessageId: number | null;
    /** Whether conversations are grouped into threads. */
    threaded?: boolean;
    /** Whether a next page can be loaded (infinite scroll). */
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
</script>

<template>
  <ThreadedMessageList
    :messages="messages"
    :selected-message-id="selectedMessageId"
    :threaded="threaded"
    list-label="Message list"
    empty-detail="Messages appear here once an account is set up and synchronized."
    :show-mark-all-read="true"
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
</template>
