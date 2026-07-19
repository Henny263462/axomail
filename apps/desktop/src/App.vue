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
import UndoToast from "./components/common/UndoToast.vue";
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
import { useMailboxActions } from "./composables/useMailboxActions";
import { useMessageActions } from "./composables/useMessageActions";
import { useMessageListLoading } from "./composables/useMessageListLoading";
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
// Loaded pages for the current mailbox only; switching mailboxes reloads.
const messages = ref<MessageListItem[]>([]);
const unifiedItems = ref<UnifiedItemDto[]>([]);
const selectedMailboxId = ref<number | null>(null);
const selectedMessageId = ref<number | null>(null);
const selectedTagId = ref<number | null>(null);
const activeView = ref<SidebarView>("mailbox");
const tagsRefreshKey = ref(0); // Bumped to reload the sidebar Tags group.
const selectionResetKey = ref(0); // Bumped to clear the list's selection.

// Next-page cursors, infinite-scroll appenders, and the threaded/flat pref.
const listLoading = useMessageListLoading({
  messages,
  unifiedItems,
  selectedMailboxId,
  activeView,
});

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

// Optimistic star / auto-read / mark-all-read, operating on the lists above.
const messageActions = useMessageActions({
  messages,
  unifiedItems,
  accounts,
  selectedMessageId,
});

// Move / copy / archive / delete-to-Trash with optimistic list splicing + undo.
const mailboxActions = useMailboxActions({
  messages,
  unifiedItems,
  accounts,
  selectedMessageId,
  dialogs,
  reload: reloadCurrentList,
  clearSelection: () => (selectionResetKey.value += 1),
});

/** Reloads the currently shown list (after an undo or a folder change). */
function reloadCurrentList(): Promise<void> {
  if (activeView.value === "unified") return loadUnified();
  if (activeView.value === "tag" && selectedTagId.value !== null)
    return selectTag(selectedTagId.value);
  if (selectedMailboxId.value !== null) return loadMessages(selectedMailboxId.value);
  return Promise.resolve();
}

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
 * Reacts to an account edit/delete or a folder mutation (Empty Trash, delete).
 * If the open mailbox's account vanished, fall back to the unified inbox;
 * otherwise reload the open list so permanently-removed messages do not linger,
 * dropping the open detail when that reload comes back empty.
 */
async function onAccountsChanged(): Promise<void> {
  await refreshAccounts();
  const mailboxId = selectedMailboxId.value;
  const accountGone =
    activeView.value === "mailbox" &&
    mailboxId !== null &&
    !accounts.value.some((a) => a.mailboxes.some((m) => m.id === mailboxId));
  if (accountGone) {
    messages.value = [];
    await selectUnified();
    return;
  }
  await reloadCurrentList();
  const list = activeView.value === "unified" ? unifiedItems : messages;
  if (list.value.length === 0) selectedMessageId.value = null;
}

async function loadMessages(mailboxId: number): Promise<void> {
  if (usingFixtures.value) {
    messages.value = fixtureMessages.filter((m) => m.mailboxId === mailboxId);
    listLoading.setMailboxCursor(null);
    return;
  }
  const page = await listMessages(mailboxId, null, 50);
  // Guard a slow response arriving after a mailbox/view switch (stale write).
  if (selectedMailboxId.value !== mailboxId || activeView.value !== "mailbox") {
    return;
  }
  messages.value = page?.items ?? [];
  listLoading.setMailboxCursor(page?.nextCursor ?? null);
}

async function loadUnified(): Promise<void> {
  const page = await listUnifiedInbox(null, 50);
  if (activeView.value !== "unified") return;
  unifiedItems.value = page?.items ?? [];
  listLoading.setUnifiedCursor(page?.nextCursor ?? null);
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
  // The list owns the multi-selection (thread-aware) and updates it on the same
  // click; here App only tracks the open message and reveals the reading pane.
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
        :threaded="listLoading.threaded.value"
        :selection-reset-key="selectionResetKey"
        @select="onSelectMessage"
        @toggle-flag="messageActions.toggleStar"
        @mark-all-read="messageActions.markListRead"
        @selection-change="listLoading.onSelectionChange"
        @bulk-action="mailboxActions.dispatch"
        @update:threaded="listLoading.onToggleThreaded"
      />
      <MailListRegion
        v-else
        :view="activeView === 'unified' ? 'unified' : 'mailbox'"
        :messages="messages"
        :unified-items="unifiedItems"
        :accounts="accounts"
        :selected-message-id="selectedMessageId"
        :selected-mailbox-id="selectedMailboxId"
        :threaded="listLoading.threaded.value"
        :has-more="listLoading.hasMore.value"
        :loading-more="listLoading.loadingMore.value"
        :selection-reset-key="selectionResetKey"
        @select="onSelectMessage"
        @toggle-flag="messageActions.toggleStar"
        @mark-all-read="onMarkAllRead"
        @load-more="listLoading.onLoadMore"
        @selection-change="listLoading.onSelectionChange"
        @bulk-action="mailboxActions.dispatch"
        @update:threaded="listLoading.onToggleThreaded"
      />
    </template>
    <template #message>
      <MessageViewPane
        v-if="messageView"
        :message-id="viewedMessageId"
        @flag-changed="messageActions.syncStar($event.messageId, $event.flagged)"
        @reply="compose.openReply"
        @reply-all="compose.openReplyAll"
        @forward="compose.openForward"
        @move="mailboxActions.requestMove([$event])"
        @copy="mailboxActions.requestCopy([$event])"
        @archive="mailboxActions.archive([$event])"
        @delete-message="mailboxActions.remove([$event])"
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
    @folder-picked="mailboxActions.onFolderPicked"
  />
  <UndoToast
    :undo-message="mailboxActions.undoState.value?.message ?? null"
    :notice="mailboxActions.notice.value"
    @undo="mailboxActions.undo"
    @dismiss="mailboxActions.dismissUndo"
  />
</template>
