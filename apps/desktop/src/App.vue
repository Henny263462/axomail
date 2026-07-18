<script setup lang="ts">
import { computed, onMounted, ref } from "vue";

import AppShell from "./components/layout/AppShell.vue";
import MessageListPane from "./components/list/MessageListPane.vue";
import MessageViewPane from "./components/message/MessageViewPane.vue";
import SidebarPane from "./components/sidebar/SidebarPane.vue";
import type { AccountSummary, MessageListItem } from "./types/mail";

const shell = ref<InstanceType<typeof AppShell> | null>(null);

// Real data arrives through typed Tauri commands once storage lands
// (work package 1c); until then these stay empty outside dev-fixtures.
const accounts = ref<AccountSummary[]>([]);
const messages = ref<MessageListItem[]>([]);
const selectedMailboxId = ref<number | null>(null);
const selectedMessageId = ref<number | null>(null);

// `dev-fixtures`: UI sample data, development builds only. The DEV
// guard is compile-time constant in release builds, so the dynamic
// import below is dropped from production bundles entirely.
onMounted(async () => {
  if (
    import.meta.env.DEV &&
    import.meta.env.VITE_AXOMAIL_DEV_FIXTURES === "1"
  ) {
    const fixtures = await import("./dev/fixtures");
    accounts.value = fixtures.fixtureAccounts;
    messages.value = fixtures.fixtureMessages;
    selectedMailboxId.value =
      fixtures.fixtureAccounts[0]?.mailboxes[0]?.id ?? null;
  }
});

const visibleMessages = computed(() =>
  messages.value.filter((m) => m.mailboxId === selectedMailboxId.value),
);

const selectedMessage = computed(
  () => messages.value.find((m) => m.id === selectedMessageId.value) ?? null,
);

function onSelectMailbox(mailboxId: number) {
  selectedMailboxId.value = mailboxId;
  selectedMessageId.value = null;
  shell.value?.showPane("list");
}

function onSelectMessage(messageId: number) {
  selectedMessageId.value = messageId;
  shell.value?.showPane("message");
}
</script>

<template>
  <AppShell ref="shell">
    <template #sidebar>
      <SidebarPane
        :accounts="accounts"
        :selected-mailbox-id="selectedMailboxId"
        @select-mailbox="onSelectMailbox"
      />
    </template>
    <template #list>
      <MessageListPane
        :messages="visibleMessages"
        :selected-message-id="selectedMessageId"
        @select="onSelectMessage"
      />
    </template>
    <template #message>
      <MessageViewPane :message="selectedMessage" />
    </template>
  </AppShell>
</template>
