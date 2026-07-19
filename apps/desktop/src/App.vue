<script setup lang="ts">
import { computed, onMounted, ref } from "vue";

import { listAccounts } from "./api/accounts";
import { cancelAllSyncJobs } from "./api/jobs";
import { listMessages } from "./api/messages";
import { getSettings } from "./api/settings";
import { syncAccount } from "./api/sync";
import { listMessagesForTag } from "./api/tags";
import { listUnifiedInbox } from "./api/unified";
import EmptyState from "./components/common/EmptyState.vue";
import AppShell from "./components/layout/AppShell.vue";
import DialogHost from "./components/layout/DialogHost.vue";
import type { AppDialogMap } from "./components/layout/dialogs";
import DraftListPane from "./components/list/DraftListPane.vue";
import MailListRegion from "./components/list/MailListRegion.vue";
import MessageListPane from "./components/list/MessageListPane.vue";
import OutboxListPane from "./components/list/OutboxListPane.vue";
import MessageViewPane from "./components/message/MessageViewPane.vue";
import SidebarPane from "./components/sidebar/SidebarPane.vue";
import type { SidebarView } from "./components/sidebar/view";
import { useComposeWorkflow } from "./composables/useComposeWorkflow";
import { useDialogs } from "./composables/useDialogs";
import { useMessageActions } from "./composables/useMessageActions";
import { useMessageSelection } from "./composables/useMessageSelection";
import { useSyncStatus } from "./composables/useSyncStatus";
import { reconcileTheme } from "./composables/useTheme";
import type {
  AccountSummary,
  MessageListItem,
  SyncEventDto,
  UnifiedItemDto,
} from "./types/mail";

const shell = ref<InstanceType<typeof AppShell> | null>(null);

const accounts = ref<AccountSummary[]>([]);
// Only the currently selected mailbox's page is held; switching mailboxes
// reloads from the backend rather than caching every mailbox client-side.
const messages = ref<MessageListItem[]>([]);
const unifiedItems = ref<UnifiedItemDto[]>([]);
const selectedMailboxId = ref<number | null>(null);
const selectedMessageId = ref<number | null>(null);
const selectedTagId = ref<number | null>(null);
const activeView = ref<SidebarView>("mailbox");
const tagsRefreshKey = ref(0); // Bumped to reload the sidebar Tags group.

// Open-state + payload for every app-level dialog; DialogHost mounts them.
const dialogs = useDialogs<AppDialogMap>();

// Composing, drafts, and the outbox (lists + view entries + composer handlers).
const compose = useComposeWorkflow({
  activeView,
  accounts,
  selectedMailboxId,
  selectedMessageId,
  dialogs,
  showList: () => shell.value?.showPane("list"),
});
const { drafts, outbox, selectedDraftId } = compose;

// Shared multi-row selection for Wave 3 tracks 3.2/3.4, fed by the active
// message-backed list's ids in order. Single-select stays the default; a track
// wires the modifier clicks that expose multi-select in the rows.
const listRowIds = computed<number[]>(() => {
  if (activeView.value === "unified") return unifiedItems.value.map((m) => m.id);
  if (activeView.value === "mailbox" || activeView.value === "tag")
    return messages.value.map((m) => m.id);
  return [];
});
const selection = useMessageSelection(listRowIds);

// Optimistic star / auto-read / mark-all-read, operating on the lists above.
const messageActions = useMessageActions({
  messages,
  unifiedItems,
  accounts,
  selectedMessageId,
});

// dev-fixtures: sample envelopes are held here so mailbox selection can
// render them without a backend. Empty (and unused) in every other mode.
let fixtureMessages: MessageListItem[] = [];
const usingFixtures = ref(false);

const { syncingMailboxIds, isSyncing, runningCount, syncError, dismissError } =
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

/** True for the message-backed views (a mailbox, the unified inbox, or a tag). */
const messageView = computed(
  () =>
    activeView.value === "mailbox" ||
    activeView.value === "unified" ||
    activeView.value === "tag",
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

/**
 * Reconciles the theme applied from the local cache (in main.ts) with the
 * authoritative backend setting. A no-op in the browser preview, where
 * getSettings resolves to null and the cached preference stands.
 */
async function syncTheme(): Promise<void> {
  const settings = await getSettings();
  if (settings === null) return;
  // Reconcile (not set): if the user already picked a theme in Settings while
  // this slow read was in flight, their choice must stand — not be stomped.
  reconcileTheme(settings.theme);
  messageActions.configureAutoRead(
    settings.autoMarkReadEnabled,
    settings.autoMarkReadDelayMs,
  );
}

onMounted(syncTheme);

/** Refreshes account summaries (e.g. unread counts) without changing focus. */
async function refreshAccounts(): Promise<void> {
  if (usingFixtures.value) return;
  const real = await listAccounts();
  if (real !== null) accounts.value = real;
}

/**
 * Handles an account edit/delete made in Settings. Re-fetches the account
 * summaries (so the sidebar reflects a renamed account) and then reconciles
 * the current selection: if the selected mailbox's account was removed, fall
 * back cleanly to the unified inbox rather than leaving a dangling selection.
 */
async function onAccountsChanged(): Promise<void> {
  await refreshAccounts();
  const mailboxId = selectedMailboxId.value;
  if (activeView.value !== "mailbox" || mailboxId === null) return;
  const stillExists = accounts.value.some((account) =>
    account.mailboxes.some((mailbox) => mailbox.id === mailboxId),
  );
  if (!stillExists) {
    messages.value = [];
    await selectUnified();
  }
}

async function loadMessages(mailboxId: number): Promise<void> {
  if (usingFixtures.value) {
    messages.value = fixtureMessages.filter((m) => m.mailboxId === mailboxId);
    return;
  }
  const page = await listMessages(mailboxId, null, 50);
  // Guard against a slow response arriving after the user has already
  // switched mailboxes or views — never overwrite the newer selection.
  if (selectedMailboxId.value !== mailboxId || activeView.value !== "mailbox") {
    return;
  }
  messages.value = page?.items ?? [];
}

async function loadUnified(): Promise<void> {
  const page = await listUnifiedInbox(null, 50);
  if (activeView.value !== "unified") return;
  unifiedItems.value = page?.items ?? [];
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

/** Filters the message list to a single tag (snapshot; not live-synced). */
async function selectTag(tagId: number): Promise<void> {
  activeView.value = "tag";
  selectedTagId.value = tagId;
  selectedMailboxId.value = null;
  selectedMessageId.value = null;
  shell.value?.showPane("list");
  const page = await listMessagesForTag(tagId);
  // Guard against a slower response arriving after another tag/view was picked.
  if (activeView.value !== "tag" || selectedTagId.value !== tagId) return;
  messages.value = page?.items ?? [];
}

function onSelectMessage(messageId: number): void {
  selectedMessageId.value = messageId;
  // Keep the shared selection set in step (single-select gesture); multi-select
  // stays dormant until a track wires modifier clicks into the list rows.
  selection.selectOnly(messageId);
  shell.value?.showPane("message");
  // Auto-mark-read (immediate or after the configured delay) is owned by the
  // message-actions composable; it no-ops for a search hit not in the lists.
  messageActions.scheduleAutoRead(messageId);
}

/** Marks every unread message in the open mailbox read (list header action). */
function onMarkAllRead(): void {
  if (selectedMailboxId.value !== null) {
    void messageActions.markAllRead(selectedMailboxId.value);
  }
}

/** Cancels every running/queued sync job from the status-bar affordance. */
async function onCancelSync(): Promise<void> {
  await cancelAllSyncJobs();
}

async function onAccountCreated(account: AccountSummary): Promise<void> {
  dialogs.close("setup");
  await loadAccounts();
  const inbox =
    account.mailboxes.find((m) => m.role === "inbox") ?? account.mailboxes[0];
  if (inbox !== undefined) await selectMailbox(inbox.id);
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
    tagsRefreshKey.value += 1; // A sync may have auto-created tags.
  }
}
</script>

<template>
  <AppShell
    ref="shell"
    :status-error="syncError"
    :syncing="isSyncing"
    :sync-count="runningCount"
    @dismiss-status-error="dismissError"
    @open-settings="dialogs.open('settings')"
    @open-activity="dialogs.open('activity')"
    @cancel-sync="onCancelSync"
  >
    <template #sidebar>
      <SidebarPane
        :accounts="accounts"
        :selected-mailbox-id="selectedMailboxId"
        :active-view="activeView"
        :syncing-account-ids="syncingAccountIds"
        :active-tag-id="activeView === 'tag' ? selectedTagId : null"
        :tags-refresh-key="tagsRefreshKey"
        @select-mailbox="selectMailbox"
        @select-unified="selectUnified"
        @add-account="dialogs.open('setup')"
        @compose="compose.openCompose"
        @sync-account="syncAccount"
        @select-drafts="compose.selectDrafts"
        @select-outbox="compose.selectOutbox"
        @select-tag="selectTag"
        @folders-changed="onAccountsChanged"
      />
    </template>
    <template #list>
      <DraftListPane
        v-if="activeView === 'drafts'"
        :drafts="drafts"
        :selected-draft-id="selectedDraftId"
        @select="compose.openDraft"
      />
      <OutboxListPane
        v-else-if="activeView === 'outbox'"
        :items="outbox"
        @retry="compose.onRetryOutbox"
        @send-now="compose.onSendOutboxNow"
        @cancel-schedule="compose.onCancelSchedule"
      />
      <MessageListPane
        v-else-if="activeView === 'tag'"
        :messages="messages"
        :selected-message-id="selectedMessageId"
        @select="onSelectMessage"
        @toggle-flag="messageActions.toggleStar"
        @mark-all-read="messageActions.markListRead"
      />
      <MailListRegion
        v-else
        :view="activeView === 'unified' ? 'unified' : 'mailbox'"
        :messages="messages"
        :unified-items="unifiedItems"
        :accounts="accounts"
        :selected-message-id="selectedMessageId"
        :selected-mailbox-id="selectedMailboxId"
        @select="onSelectMessage"
        @toggle-flag="messageActions.toggleStar"
        @mark-all-read="onMarkAllRead"
      />
    </template>
    <template #message>
      <MessageViewPane
        v-if="messageView"
        :message-id="viewedMessageId"
        @flag-changed="messageActions.syncStar($event.messageId, $event.flagged)"
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
  <DialogHost
    :dialogs="dialogs"
    :accounts="accounts"
    @account-created="onAccountCreated"
    @composer-sent="compose.onComposerSent"
    @composer-saved="compose.onComposerSaved"
    @accounts-changed="onAccountsChanged"
    @tags-changed="tagsRefreshKey += 1"
    @settings-saved="syncTheme"
  />
</template>
