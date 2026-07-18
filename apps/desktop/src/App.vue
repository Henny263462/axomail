<script setup lang="ts">
import { computed, onMounted, ref } from "vue";

import { listAccounts } from "./api/accounts";
import {
  deleteDraft,
  listDrafts,
  listOutbox,
  retryOutbox,
} from "./api/compose";
import { listMessages, markMessageSeen } from "./api/messages";
import { syncAccount } from "./api/sync";
import { listUnifiedInbox } from "./api/unified";
import ComposerDialog from "./components/compose/ComposerDialog.vue";
import EmptyState from "./components/common/EmptyState.vue";
import AppShell from "./components/layout/AppShell.vue";
import DraftListPane from "./components/list/DraftListPane.vue";
import MailListRegion from "./components/list/MailListRegion.vue";
import OutboxListPane from "./components/list/OutboxListPane.vue";
import MessageViewPane from "./components/message/MessageViewPane.vue";
import SettingsDialog from "./components/settings/SettingsDialog.vue";
import AccountSetupDialog from "./components/setup/AccountSetupDialog.vue";
import SidebarPane from "./components/sidebar/SidebarPane.vue";
import type { SidebarView } from "./components/sidebar/view";
import { useSyncStatus } from "./composables/useSyncStatus";
import type {
  AccountSummary,
  DraftDto,
  MessageListItem,
  OutboxItemDto,
  SyncEventDto,
  UnifiedItemDto,
} from "./types/mail";

const shell = ref<InstanceType<typeof AppShell> | null>(null);

const accounts = ref<AccountSummary[]>([]);
// Only the currently selected mailbox's page is held; switching mailboxes
// reloads from the backend rather than caching every mailbox client-side.
const messages = ref<MessageListItem[]>([]);
const unifiedItems = ref<UnifiedItemDto[]>([]);
const drafts = ref<DraftDto[]>([]);
const outbox = ref<OutboxItemDto[]>([]);
const selectedMailboxId = ref<number | null>(null);
const selectedMessageId = ref<number | null>(null);
const selectedDraftId = ref<number | null>(null);
const activeView = ref<SidebarView>("mailbox");
const setupOpen = ref(false);
const settingsOpen = ref(false);
const composerOpen = ref(false);
const composerDraft = ref<DraftDto | null>(null);
const composerAccountId = ref(0);

// dev-fixtures: sample envelopes are held here so mailbox selection can
// render them without a backend. Empty (and unused) in every other mode.
let fixtureMessages: MessageListItem[] = [];
const usingFixtures = ref(false);

const { syncingMailboxIds, syncError, dismissError } =
  useSyncStatus(onSyncEvent);

/** Account ids that have at least one mailbox currently syncing. */
const syncingAccountIds = computed(() =>
  accounts.value
    .filter((account) =>
      account.mailboxes.some((mailbox) =>
        syncingMailboxIds.value.has(mailbox.id),
      ),
    )
    .map((account) => account.id),
);

/** Account a new message is composed from: the selected one, else the first. */
const composeAccountId = computed<number | null>(() => {
  const owning = accounts.value.find((account) =>
    account.mailboxes.some((mailbox) => mailbox.id === selectedMailboxId.value),
  );
  return owning?.id ?? accounts.value[0]?.id ?? null;
});

/** True for the message-backed views (a single mailbox or the unified inbox). */
const messageView = computed(
  () => activeView.value === "mailbox" || activeView.value === "unified",
);

const viewedMessageId = computed(() =>
  messageView.value ? selectedMessageId.value : null,
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

/** Refreshes account summaries (e.g. unread counts) without changing focus. */
async function refreshAccounts(): Promise<void> {
  if (usingFixtures.value) return;
  const real = await listAccounts();
  if (real !== null) accounts.value = real;
}

async function loadMessages(mailboxId: number): Promise<void> {
  if (usingFixtures.value) {
    messages.value = fixtureMessages.filter((m) => m.mailboxId === mailboxId);
    return;
  }
  const page = await listMessages(mailboxId, null, 50);
  messages.value = page?.items ?? [];
}

async function loadUnified(): Promise<void> {
  const page = await listUnifiedInbox(null, 50);
  unifiedItems.value = page?.items ?? [];
}

async function loadDrafts(): Promise<void> {
  drafts.value = (await listDrafts()) ?? [];
}

async function loadOutbox(): Promise<void> {
  outbox.value = (await listOutbox()) ?? [];
}

async function selectMailbox(mailboxId: number): Promise<void> {
  activeView.value = "mailbox";
  selectedMailboxId.value = mailboxId;
  selectedMessageId.value = null;
  shell.value?.showPane("list");
  await loadMessages(mailboxId);
}

async function selectUnified(): Promise<void> {
  activeView.value = "unified";
  selectedMailboxId.value = null;
  selectedMessageId.value = null;
  shell.value?.showPane("list");
  await loadUnified();
}

async function selectDrafts(): Promise<void> {
  activeView.value = "drafts";
  selectedMessageId.value = null;
  shell.value?.showPane("list");
  await loadDrafts();
}

async function selectOutbox(): Promise<void> {
  activeView.value = "outbox";
  selectedMessageId.value = null;
  shell.value?.showPane("list");
  await loadOutbox();
}

function onSelectMessage(messageId: number): void {
  selectedMessageId.value = messageId;
  shell.value?.showPane("message");
  // The selected id may come from the mailbox list, the unified inbox, or a
  // search result; only the first two carry a seen flag to update optimistically.
  const message =
    messages.value.find((m) => m.id === messageId) ??
    unifiedItems.value.find((m) => m.id === messageId);
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

function openSettings(): void {
  settingsOpen.value = true;
}

function closeSettings(): void {
  settingsOpen.value = false;
}

async function onAccountCreated(account: AccountSummary): Promise<void> {
  setupOpen.value = false;
  await loadAccounts();
  const inbox =
    account.mailboxes.find((m) => m.role === "inbox") ?? account.mailboxes[0];
  if (inbox !== undefined) await selectMailbox(inbox.id);
}

function openCompose(): void {
  const id = composeAccountId.value;
  if (id === null) return;
  composerAccountId.value = id;
  composerDraft.value = null;
  composerOpen.value = true;
}

function openDraft(draftId: number): void {
  const draft = drafts.value.find((d) => d.id === draftId);
  if (draft === undefined) return;
  selectedDraftId.value = draftId;
  composerAccountId.value = draft.accountId;
  composerDraft.value = draft;
  composerOpen.value = true;
}

function closeComposer(): void {
  composerOpen.value = false;
  composerDraft.value = null;
}

async function onComposerSent(payload: { draftId: number | null }): Promise<void> {
  closeComposer();
  // A draft that was sent is removed client-side (send then delete).
  if (payload.draftId !== null) await deleteDraft(payload.draftId);
  if (activeView.value === "drafts") await loadDrafts();
  if (activeView.value === "outbox") await loadOutbox();
}

async function onComposerSaved(): Promise<void> {
  if (activeView.value === "drafts") await loadDrafts();
}

async function onSyncAccount(accountId: number): Promise<void> {
  await syncAccount(accountId);
}

async function onRetryOutbox(outboxId: number): Promise<void> {
  await retryOutbox(outboxId);
  await loadOutbox();
}

/** Reacts to sync events after {@link useSyncStatus} updates its own state. */
function onSyncEvent(event: SyncEventDto): void {
  if (event.kind === "batchIngested") {
    if (
      activeView.value === "mailbox" &&
      event.mailboxId !== null &&
      event.mailboxId === selectedMailboxId.value
    ) {
      void loadMessages(event.mailboxId);
    } else if (activeView.value === "unified") {
      // The unified inbox spans every account, so any ingest may add to it.
      void loadUnified();
    }
  }
  if (event.kind === "jobFinished") {
    void refreshAccounts();
  }
}
</script>

<template>
  <AppShell
    ref="shell"
    :status-error="syncError"
    @dismiss-status-error="dismissError"
    @open-settings="openSettings"
  >
    <template #sidebar>
      <SidebarPane
        :accounts="accounts"
        :selected-mailbox-id="selectedMailboxId"
        :active-view="activeView"
        :syncing-account-ids="syncingAccountIds"
        @select-mailbox="selectMailbox"
        @select-unified="selectUnified"
        @add-account="openSetup"
        @compose="openCompose"
        @sync-account="onSyncAccount"
        @select-drafts="selectDrafts"
        @select-outbox="selectOutbox"
      />
    </template>
    <template #list>
      <DraftListPane
        v-if="activeView === 'drafts'"
        :drafts="drafts"
        :selected-draft-id="selectedDraftId"
        @select="openDraft"
      />
      <OutboxListPane
        v-else-if="activeView === 'outbox'"
        :items="outbox"
        @retry="onRetryOutbox"
      />
      <MailListRegion
        v-else
        :view="activeView === 'unified' ? 'unified' : 'mailbox'"
        :messages="messages"
        :unified-items="unifiedItems"
        :accounts="accounts"
        :selected-message-id="selectedMessageId"
        @select="onSelectMessage"
      />
    </template>
    <template #message>
      <MessageViewPane
        v-if="messageView"
        :message-id="viewedMessageId"
      />
      <EmptyState
        v-else-if="activeView === 'drafts'"
        title="Drafts"
        detail="Select a draft to continue editing it."
      />
      <EmptyState
        v-else
        title="Outbox"
        detail="Messages waiting to be delivered are listed here."
      />
    </template>
  </AppShell>
  <AccountSetupDialog
    v-if="setupOpen"
    @close="closeSetup"
    @created="onAccountCreated"
  />
  <ComposerDialog
    v-if="composerOpen"
    :account-id="composerAccountId"
    :draft="composerDraft"
    @close="closeComposer"
    @sent="onComposerSent"
    @saved="onComposerSaved"
  />
  <SettingsDialog
    v-if="settingsOpen"
    :accounts="accounts"
    @close="closeSettings"
  />
</template>
