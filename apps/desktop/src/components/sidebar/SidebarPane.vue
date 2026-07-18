<script setup lang="ts">
import type { AccountSummary } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";

defineProps<{
  accounts: AccountSummary[];
  selectedMailboxId: number | null;
}>();

const emit = defineEmits<{
  selectMailbox: [mailboxId: number];
  addAccount: [];
}>();
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
    <EmptyState
      v-if="accounts.length === 0"
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
        <h2
          class="account-name"
          :title="account.email"
        >
          {{ account.displayName }}
        </h2>
        <ul>
          <li
            v-for="mailbox in account.mailboxes"
            :key="mailbox.id"
          >
            <button
              type="button"
              class="mailbox"
              :class="{ selected: mailbox.id === selectedMailboxId }"
              :aria-current="mailbox.id === selectedMailboxId ? 'true' : undefined"
              @click="emit('selectMailbox', mailbox.id)"
            >
              <span class="mailbox-name">{{ mailbox.name }}</span>
              <span
                v-if="mailbox.unreadCount > 0"
                class="unread"
                :aria-label="`${mailbox.unreadCount} unread`"
              >
                {{ mailbox.unreadCount }}
              </span>
            </button>
          </li>
        </ul>
      </section>
    </template>
  </nav>
</template>

<style scoped>
.sidebar {
  height: 100%;
  overflow-y: auto;
  background: var(--color-bg-subtle);
  border-right: 1px solid var(--color-border);
  padding: var(--space-3);
}

.add-account {
  display: block;
  width: 100%;
  padding: var(--space-2);
  margin-bottom: var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text);
  background: var(--color-bg);
  transition: background var(--transition-fast);
}

.add-account:hover {
  background: var(--color-bg-hover);
}

.account-name {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: var(--space-2) var(--space-2) var(--space-1);
}

.mailbox {
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

.mailbox:hover {
  background: var(--color-bg-hover);
}

.mailbox.selected {
  background: var(--color-bg-selected);
  color: var(--color-text);
  font-weight: var(--weight-medium);
}

.unread {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-accent);
}
</style>
