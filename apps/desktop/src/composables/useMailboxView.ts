/**
 * The main content area's data and navigation: loading accounts (with the
 * dev-fixtures fallback), the message-backed view routing (a single mailbox,
 * the unified inbox, or a tag filter), first-page list loading, and the
 * account/folder-change reconciliation.
 *
 * Extracted from `App.vue` so the shell stays well under the file-size gate and
 * the Wave 4 tracks have headroom to add wiring. Behaviour is byte-for-byte the
 * same as the previous inline code; only the location changed. It reads and
 * writes the same reactive refs `App.vue` owns (props-down / events-up; no
 * store), exactly like the sibling composables.
 */
import { computed, onMounted, type Ref } from "vue";

import { listAccounts } from "../api/accounts";
import { listMessages } from "../api/messages";
import { listMessagesForTag } from "../api/tags";
import { listUnifiedInbox } from "../api/unified";
import type { AppDialogMap } from "../components/layout/dialogs";
import type { SidebarView } from "../components/sidebar/view";
import type {
  AccountSummary,
  MessageListItem,
  UnifiedItemDto,
} from "../types/mail";
import type { DialogController } from "./useDialogs";

/** Page size for the mailbox and unified first-page loads. */
const PAGE_SIZE = 50;

/**
 * The App-level state and collaborators this composable reads and drives. The
 * refs are the ones `App.vue` owns; the callbacks reach the shell and the
 * sibling composables it wires alongside this one.
 */
interface Deps {
  /** Account summaries, replaced on load/refresh and read by the sidebar. */
  accounts: Ref<AccountSummary[]>;
  /** Current single-mailbox / tag-filter list (replaced on each load). */
  messages: Ref<MessageListItem[]>;
  /** Current unified-inbox list (replaced on each load). */
  unifiedItems: Ref<UnifiedItemDto[]>;
  /** The selected mailbox, or null in the unified / tag / compose views. */
  selectedMailboxId: Ref<number | null>;
  /** The open message; cleared when switching mailbox or view. */
  selectedMessageId: Ref<number | null>;
  /** The active tag in the tag-filter view, or null. */
  selectedTagId: Ref<number | null>;
  /** The active sidebar view. */
  activeView: Ref<SidebarView>;
  /** Records the mailbox list's next-page cursor (owned by list-loading). */
  setMailboxCursor: (cursor: string | null) => void;
  /** Records the unified list's next-page cursor (owned by list-loading). */
  setUnifiedCursor: (cursor: string | null) => void;
  /** App dialog controller, used to close the setup dialog after a create. */
  dialogs: DialogController<AppDialogMap>;
  /** Reveals a pane on the narrow, stacked layout (`shell.showPane`). */
  showPane: (pane: "list" | "message") => void;
  /** Schedules auto-mark-read for a newly opened message (message-actions). */
  scheduleAutoRead: (messageId: number) => void;
}

export function useMailboxView(deps: Deps) {
  // dev-fixtures: sample envelopes are held here so mailbox selection can
  // render them without a backend. Empty (and unused) in every other mode.
  let fixtureMessages: MessageListItem[] = [];
  let usingFixtures = false;

  /**
   * True for the message-backed views: a mailbox, the unified inbox, a tag
   * filter, or an advanced/saved-search results list. Each opens a message the
   * same way, so the reading pane shows the selected id in all of them.
   */
  const messageView = computed(
    () =>
      deps.activeView.value === "mailbox" ||
      deps.activeView.value === "unified" ||
      deps.activeView.value === "tag" ||
      deps.activeView.value === "search",
  );

  const viewedMessageId = computed(() =>
    messageView.value ? deps.selectedMessageId.value : null,
  );

  /**
   * Loads accounts from the backend. Falls back to dev-fixtures only when the
   * flag is set AND no real accounts exist, so fixtures never mask real data.
   */
  async function loadAccounts(): Promise<void> {
    const real = await listAccounts();
    if (real !== null && real.length > 0) {
      deps.accounts.value = real;
      usingFixtures = false;
      return;
    }
    if (
      import.meta.env.DEV &&
      import.meta.env.VITE_AXOMAIL_DEV_FIXTURES === "1"
    ) {
      const fixtures = await import("../dev/fixtures");
      deps.accounts.value = fixtures.fixtureAccounts;
      fixtureMessages = fixtures.fixtureMessages;
      usingFixtures = true;
      const first = fixtures.fixtureAccounts[0]?.mailboxes[0];
      if (first !== undefined) await selectMailbox(first.id);
      return;
    }
    deps.accounts.value = real ?? [];
    usingFixtures = false;
  }

  onMounted(loadAccounts);

  /** Refreshes account summaries (e.g. unread counts) without changing focus. */
  async function refreshAccounts(): Promise<void> {
    if (usingFixtures) return;
    const real = await listAccounts();
    if (real !== null) deps.accounts.value = real;
  }

  /**
   * Reacts to an account edit/delete or a folder mutation (Empty Trash, delete).
   * If the open mailbox's account vanished, fall back to the unified inbox;
   * otherwise reload the open list so permanently-removed messages do not linger,
   * dropping the open detail when that reload comes back empty.
   */
  async function onAccountsChanged(): Promise<void> {
    await refreshAccounts();
    const mailboxId = deps.selectedMailboxId.value;
    const accountGone =
      deps.activeView.value === "mailbox" &&
      mailboxId !== null &&
      !deps.accounts.value.some((a) =>
        a.mailboxes.some((m) => m.id === mailboxId),
      );
    if (accountGone) {
      deps.messages.value = [];
      await selectUnified();
      return;
    }
    await reloadCurrentList();
    const list =
      deps.activeView.value === "unified" ? deps.unifiedItems : deps.messages;
    if (list.value.length === 0) deps.selectedMessageId.value = null;
  }

  async function loadMessages(mailboxId: number): Promise<void> {
    if (usingFixtures) {
      deps.messages.value = fixtureMessages.filter(
        (m) => m.mailboxId === mailboxId,
      );
      deps.setMailboxCursor(null);
      return;
    }
    const page = await listMessages(mailboxId, null, PAGE_SIZE);
    // Guard a slow response arriving after a mailbox/view switch (stale write).
    if (
      deps.selectedMailboxId.value !== mailboxId ||
      deps.activeView.value !== "mailbox"
    ) {
      return;
    }
    deps.messages.value = page?.items ?? [];
    deps.setMailboxCursor(page?.nextCursor ?? null);
  }

  async function loadUnified(): Promise<void> {
    const page = await listUnifiedInbox(null, PAGE_SIZE);
    if (deps.activeView.value !== "unified") return;
    deps.unifiedItems.value = page?.items ?? [];
    deps.setUnifiedCursor(page?.nextCursor ?? null);
  }

  async function selectMailbox(mailboxId: number): Promise<void> {
    deps.activeView.value = "mailbox";
    deps.selectedMailboxId.value = mailboxId;
    deps.selectedMessageId.value = null;
    deps.showPane("list");
    await loadMessages(mailboxId);
  }

  async function selectUnified(): Promise<void> {
    deps.activeView.value = "unified";
    deps.selectedMailboxId.value = null;
    deps.selectedMessageId.value = null;
    deps.showPane("list");
    await loadUnified();
  }

  /** Filters the message list to a single tag (snapshot; not live-synced). */
  async function selectTag(tagId: number): Promise<void> {
    deps.activeView.value = "tag";
    deps.selectedTagId.value = tagId;
    deps.selectedMailboxId.value = null;
    deps.selectedMessageId.value = null;
    deps.showPane("list");
    const page = await listMessagesForTag(tagId);
    // Guard against a slower response arriving after another tag/view was picked.
    if (deps.activeView.value !== "tag" || deps.selectedTagId.value !== tagId)
      return;
    deps.messages.value = page?.items ?? [];
  }

  /** Reloads the currently shown list (after an undo or a folder change). */
  function reloadCurrentList(): Promise<void> {
    if (deps.activeView.value === "unified") return loadUnified();
    if (deps.activeView.value === "tag" && deps.selectedTagId.value !== null)
      return selectTag(deps.selectedTagId.value);
    if (deps.selectedMailboxId.value !== null)
      return loadMessages(deps.selectedMailboxId.value);
    return Promise.resolve();
  }

  function onSelectMessage(messageId: number): void {
    deps.selectedMessageId.value = messageId;
    // The list owns the multi-selection (thread-aware) and updates it on the same
    // click; here App only tracks the open message and reveals the reading pane.
    deps.showPane("message");
    // Auto-mark-read (immediate or after the configured delay) is owned by the
    // message-actions composable; it no-ops for a search hit not in the lists.
    deps.scheduleAutoRead(messageId);
  }

  async function onAccountCreated(account: AccountSummary): Promise<void> {
    deps.dialogs.close("setup");
    await loadAccounts();
    const inbox =
      account.mailboxes.find((m) => m.role === "inbox") ?? account.mailboxes[0];
    if (inbox !== undefined) await selectMailbox(inbox.id);
  }

  return {
    messageView,
    viewedMessageId,
    loadMessages,
    loadUnified,
    selectMailbox,
    selectUnified,
    selectTag,
    reloadCurrentList,
    onSelectMessage,
    refreshAccounts,
    onAccountsChanged,
    onAccountCreated,
  };
}
