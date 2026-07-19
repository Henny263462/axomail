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
import { watch } from "vue";

import { useSearch } from "../../composables/useSearch";
import type {
  AccountSummary,
  MessageListItem,
  UnifiedItemDto,
} from "../../types/mail";
import MessageListPane from "./MessageListPane.vue";
import SearchBox from "./SearchBox.vue";
import SearchResultsPane from "./SearchResultsPane.vue";
import UnifiedListPane from "./UnifiedListPane.vue";

const props = defineProps<{
  view: "mailbox" | "unified";
  messages: MessageListItem[];
  unifiedItems: UnifiedItemDto[];
  accounts: AccountSummary[];
  selectedMessageId: number | null;
  /** Identity of the shown mailbox; switching it resets an active search. */
  selectedMailboxId: number | null;
}>();

const emit = defineEmits<{
  select: [messageId: number];
  toggleFlag: [messageId: number];
  markAllRead: [];
}>();

const { query, results, active, searching, clear } = useSearch();

// A stale result list must never hide a freshly selected mailbox or the
// unified inbox: any change of the shown list resets the search.
watch(
  () => [props.view, props.selectedMailboxId],
  () => clear(),
);
</script>

<template>
  <div class="region">
    <SearchBox
      v-model="query"
      @escape="clear"
    />
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
      @select="emit('select', $event)"
      @toggle-flag="emit('toggleFlag', $event)"
    />
    <MessageListPane
      v-else
      :messages="messages"
      :selected-message-id="selectedMessageId"
      @select="emit('select', $event)"
      @toggle-flag="emit('toggleFlag', $event)"
      @mark-all-read="emit('markAllRead')"
    />
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
</style>
