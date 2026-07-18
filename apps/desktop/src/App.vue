<script setup lang="ts">
import { computed, onMounted, ref } from "vue";

import { listAccounts } from "./api/accounts";
import { listMessages, markMessageSeen } from "./api/messages";
import AppShell from "./components/layout/AppShell.vue";
import MessageListPane from "./components/list/MessageListPane.vue";
import MessageViewPane from "./components/message/MessageViewPane.vue";
import AccountSetupDialog from "./components/setup/AccountSetupDialog.vue";
import SidebarPane from "./components/sidebar/SidebarPane.vue";
import type { AccountSummary, MessageListItem } from "./types/mail";

const shell = ref<InstanceType<typeof AppShell> | null>(null);

const accounts = ref<AccountSummary[]>([]);
// Only the currently selected mailbox's page is held; switching mailboxes
// reloads from the backend rather than caching every mailbox client-side.
const messages = ref<MessageListItem[]>([]);
const selectedMailboxId = ref<number | null>(null);
const selectedMessageId = ref<number | null>(null);
const setupOpen = ref(false);

// dev-fixtures: sample envelopes are held here so mailbox selection can
// render them without a backend. Empty (and unused) in every other mode.
let fixtureMessages: MessageListItem[] = [];
const usingFixtures = ref(false);

const selectedMessage = computed(
  () => messages.value.find((m) => m.id === selectedMessageId.value) ?? null,
);

/**
 * Loads accounts from the backend. Falls back to dev-fixtures only when the
 * flag is set AND no real accounts exist, so fixtures never mask real data.
 */
async function loadAccounts(): Promise<void> {
  const real = await listAccounts();
  if (real !== null && real.length > 0) {
    accounts.value = real;
    usingFixtures.value = false;
    return;
  }
  if (
    import.meta.env.DEV &&
    import.meta.env.VITE_AXOMAIL_DEV_FIXTURES === "1"
  ) {
    const fixtures = await import("./dev/fixtures");
    accounts.value = fixtures.fixtureAccounts;
    fixtureMessages = fixtures.fixtureMessages;
    usingFixtures.value = true;
    const first = fixtures.fixtureAccounts[0]?.mailboxes[0];
    if (first !== undefined) await selectMailbox(first.id);
    return;
  }
  accounts.value = real ?? [];
  usingFixtures.value = false;
}

onMounted(loadAccounts);

async function loadMessages(mailboxId: number): Promise<void> {
  if (usingFixtures.value) {
    messages.value = fixtureMessages.filter((m) => m.mailboxId === mailboxId);
    return;
  }
  const page = await listMessages(mailboxId, null, 50);
  messages.value = page?.items ?? [];
}

async function selectMailbox(mailboxId: number): Promise<void> {
  selectedMailboxId.value = mailboxId;
  selectedMessageId.value = null;
  shell.value?.showPane("list");
  await loadMessages(mailboxId);
}

function onSelectMessage(messageId: number): void {
  selectedMessageId.value = messageId;
  shell.value?.showPane("message");
  const message = messages.value.find((m) => m.id === messageId);
  if (message !== undefined && !message.seen) {
    // Optimistic: reflect the read state immediately. In browser preview
    // markMessageSeen resolves null with nothing to persist; that is fine.
    message.seen = true;
    void markMessageSeen(messageId, true);
  }
}

function openSetup(): void {
  setupOpen.value = true;
}

function closeSetup(): void {
  setupOpen.value = false;
}

async function onAccountCreated(account: AccountSummary): Promise<void> {
  setupOpen.value = false;
  await loadAccounts();
  const inbox =
    account.mailboxes.find((m) => m.role === "inbox") ?? account.mailboxes[0];
  if (inbox !== undefined) await selectMailbox(inbox.id);
}
</script>

<template>
  <AppShell ref="shell">
    <template #sidebar>
      <SidebarPane
        :accounts="accounts"
        :selected-mailbox-id="selectedMailboxId"
        @select-mailbox="selectMailbox"
        @add-account="openSetup"
      />
    </template>
    <template #list>
      <MessageListPane
        :messages="messages"
        :selected-message-id="selectedMessageId"
        @select="onSelectMessage"
      />
    </template>
    <template #message>
      <MessageViewPane :message="selectedMessage" />
    </template>
  </AppShell>
  <AccountSetupDialog
    v-if="setupOpen"
    @close="closeSetup"
    @created="onAccountCreated"
  />
</template>
