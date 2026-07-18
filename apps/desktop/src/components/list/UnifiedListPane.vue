<script setup lang="ts">
import { computed } from "vue";

import type { AccountSummary, UnifiedItemDto } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";
import MessageListItem from "./MessageListItem.vue";

const props = defineProps<{
  items: UnifiedItemDto[];
  /** Accounts, used to label each item with its account display name. */
  accounts: AccountSummary[];
  selectedMessageId: number | null;
}>();

const emit = defineEmits<{
  select: [messageId: number];
}>();

const namesByAccount = computed(() => {
  const map = new Map<number, string>();
  for (const account of props.accounts) map.set(account.id, account.displayName);
  return map;
});

function accountName(accountId: number): string {
  return namesByAccount.value.get(accountId) ?? `Account ${accountId}`;
}
</script>

<template>
  <section
    class="list"
    aria-label="Unified inbox"
  >
    <EmptyState
      v-if="items.length === 0"
      title="No messages"
      detail="Messages from every account's inbox appear here once synchronized."
    />
    <ul v-else>
      <MessageListItem
        v-for="item in items"
        :key="item.id"
        :message="item"
        :account-name="accountName(item.accountId)"
        :selected="item.id === selectedMessageId"
        @select="emit('select', $event)"
      />
    </ul>
  </section>
</template>

<style scoped>
.list {
  height: 100%;
  overflow-y: auto;
  border-right: 1px solid var(--color-border);
  background: var(--color-bg);
}
</style>
