<script setup lang="ts">
import { computed, onMounted, ref } from "vue";

import { cancelAllSyncJobs } from "./api/jobs";
import { getLanguage, getSettings } from "./api/settings";
import { syncAccount } from "./api/sync";
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
import AdvancedSearchResultsPane from "./components/search/AdvancedSearchResultsPane.vue";
import SidebarPane from "./components/sidebar/SidebarPane.vue";
import type { SidebarView } from "./components/sidebar/view";
import { useAppShortcuts } from "./composables/useAppShortcuts";
import { useComposeWorkflow } from "./composables/useComposeWorkflow";
import { useDialogs } from "./composables/useDialogs";
import { useMailboxActions } from "./composables/useMailboxActions";
import { useMailboxView } from "./composables/useMailboxView";
import { useMessageActions } from "./composables/useMessageActions";
import { useMessageListLoading } from "./composables/useMessageListLoading";
import { useSavedSearch } from "./composables/useSavedSearch";
import { useSyncStatus } from "./composables/useSyncStatus";
import { reconcileTheme } from "./composables/useTheme";
import { reconcileLanguage } from "./i18n";
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

// Optimistic star / auto-read / mark-all-read, operating on the shared lists.
const messageActions = useMessageActions({
  messages,
  unifiedItems,
  accounts,
  selectedMessageId,
});

// Account loading + message-backed view routing + first-page list loading.
const view = useMailboxView({
  accounts,
  messages,
  unifiedItems,
  selectedMailboxId,
  selectedMessageId,
  selectedTagId,
  activeView,
  setMailboxCursor: listLoading.setMailboxCursor,
  setUnifiedCursor: listLoading.setUnifiedCursor,
  dialogs,
  showPane: (pane) => shell.value?.showPane(pane),
  scheduleAutoRead: messageActions.scheduleAutoRead,
});

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

// Move / copy / archive / delete-to-Trash / junk with optimistic splicing.
const mailboxActions = useMailboxActions({
  messages,
  unifiedItems,
  accounts,
  selectedMessageId,
  dialogs,
  reload: view.reloadCurrentList,
  clearSelection: () => (selectionResetKey.value += 1),
});

// App-wide command registry + global keyboard/shortcut wiring (the standard
// mail command map and the address-book/junk context commands), all modal-gated
// so no global key acts on the background behind a dialog. Returns the modal
// predicate the message pane reads. Extracted to keep the shell under the gate.
const { isModalOpen } = useAppShortcuts({
  dialogs,
  accounts,
  selectedMessageId,
  showPane: (pane) => shell.value?.showPane(pane),
  view,
  compose,
  mailboxActions,
  messageActions,
});

// Advanced-search results + saved-search (virtual folder) state. Registers the
// "Advanced search" command against the registry provided by useAppShortcuts.
const savedSearch = useSavedSearch({
  activeView,
  selectedMessageId,
  dialogs,
  showList: () => shell.value?.showPane("list"),
  isModalOpen,
});

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

/**
 * Reconciles the theme and language applied from the local caches (in main.ts)
 * with the authoritative backend settings. Reconcile (not set): if the user
 * already picked either in Settings while these slow reads were in flight, that
 * live choice must stand — not be stomped. A no-op in the browser preview,
 * where the reads resolve to null and the cached preferences stand.
 */
async function syncStartupPreferences(): Promise<void> {
  const language = await getLanguage();
  if (language !== null) reconcileLanguage(language);
  const settings = await getSettings();
  if (settings === null) return;
  reconcileTheme(settings.theme);
  messageActions.configureAutoRead(
    settings.autoMarkReadEnabled,
    settings.autoMarkReadDelayMs,
  );
}

onMounted(syncStartupPreferences);

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

/** Reacts to sync events after {@link useSyncStatus} updates its own state. */
function onSyncEvent(event: SyncEventDto): void {
  if (event.kind === "batchIngested") {
    if (
      activeView.value === "mailbox" &&
      event.mailboxId !== null &&
      event.mailboxId === selectedMailboxId.value
    ) {
      void view.loadMessages(event.mailboxId);
    } else if (activeView.value === "unified") {
      // The unified inbox spans every account, so any ingest may add to it.
      void view.loadUnified();
    }
  }
  if (event.kind === "jobFinished") {
    void view.refreshAccounts();
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
        :active-saved-search-id="activeView === 'search' ? savedSearch.activeSavedSearchId.value : null"
        :saved-search-refresh-key="savedSearch.refreshKey.value"
        @select-mailbox="view.selectMailbox"
        @select-unified="view.selectUnified"
        @add-account="dialogs.open('setup')"
        @compose="compose.openCompose"
        @sync-account="syncAccount"
        @select-drafts="compose.selectDrafts"
        @select-outbox="compose.selectOutbox"
        @select-tag="view.selectTag"
        @folders-changed="view.onAccountsChanged"
        @select-saved-search="savedSearch.openSaved"
        @edit-saved-search="savedSearch.openDialog"
        @saved-searches-changed="savedSearch.refresh"
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
        @select="view.onSelectMessage"
        @toggle-flag="messageActions.toggleStar"
        @mark-all-read="messageActions.markListRead"
        @selection-change="listLoading.onSelectionChange"
        @bulk-action="mailboxActions.dispatch"
        @update:threaded="listLoading.onToggleThreaded"
      />
      <AdvancedSearchResultsPane
        v-else-if="activeView === 'search'"
        :title="savedSearch.resultsTitle.value"
        :results="savedSearch.results.value"
        :searching="savedSearch.running.value"
        :selected-message-id="selectedMessageId"
        @select="view.onSelectMessage"
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
        @select="view.onSelectMessage"
        @toggle-flag="messageActions.toggleStar"
        @mark-all-read="onMarkAllRead"
        @load-more="listLoading.onLoadMore"
        @selection-change="listLoading.onSelectionChange"
        @bulk-action="mailboxActions.dispatch"
        @update:threaded="listLoading.onToggleThreaded"
        @advanced-search="savedSearch.openDialog()"
      />
    </template>
    <template #message>
      <MessageViewPane
        v-if="view.messageView.value"
        :message-id="view.viewedMessageId.value"
        :is-modal-open="isModalOpen"
        @flag-changed="messageActions.syncStar($event.messageId, $event.flagged)"
        @reply="compose.openReply"
        @reply-all="compose.openReplyAll"
        @forward="compose.openForward"
        @move="mailboxActions.requestMove([$event])"
        @copy="mailboxActions.requestCopy([$event])"
        @archive="mailboxActions.archive([$event])"
        @junk="mailboxActions.junk([$event])"
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
    @account-created="view.onAccountCreated"
    @composer-sent="compose.onComposerSent"
    @composer-saved="compose.onComposerSaved"
    @accounts-changed="view.onAccountsChanged"
    @tags-changed="tagsRefreshKey += 1"
    @settings-saved="syncStartupPreferences"
    @folder-picked="mailboxActions.onFolderPicked"
    @advanced-search-run="savedSearch.showResults"
    @saved-searches-changed="savedSearch.refresh"
  />
  <UndoToast
    :undo-message="mailboxActions.undoState.value?.message ?? null"
    :notice="mailboxActions.notice.value"
    @undo="mailboxActions.undo"
    @dismiss="mailboxActions.dismissUndo"
  />
</template>
