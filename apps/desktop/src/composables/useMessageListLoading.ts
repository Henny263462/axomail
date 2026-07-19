/**
 * List-loading state for the message-backed views: next-page cursors and the
 * infinite-scroll appenders, the persisted threaded/flat preference, and the
 * multi-selection projection surfaced for the bulk-actions track (3.2).
 *
 * Extracted from `App.vue` so the shell stays under the file-size gate; it reads
 * and appends the same `messages`/`unifiedItems` refs App owns (props-down /
 * events-up; no store).
 */
import { computed, onMounted, ref, type Ref } from "vue";

import { getListThreaded, setListThreaded } from "../api/listPrefs";
import { listMessages } from "../api/messages";
import { listUnifiedInbox } from "../api/unified";
import type { SidebarView } from "../components/sidebar/view";
import type { MessageListItem, UnifiedItemDto } from "../types/mail";

/** Page size for the mailbox and unified list loads (first page and beyond). */
const PAGE_SIZE = 50;

interface Deps {
  messages: Ref<MessageListItem[]>;
  unifiedItems: Ref<UnifiedItemDto[]>;
  selectedMailboxId: Ref<number | null>;
  activeView: Ref<SidebarView>;
}

export function useMessageListLoading(deps: Deps) {
  const messagesCursor = ref<string | null>(null);
  const unifiedCursor = ref<string | null>(null);
  const loadingMore = ref(false);
  const threaded = ref(false);
  // Message ids of the current multi-selection, surfaced by the list for the
  // bulk-actions track (3.2) to consume.
  const selectedMessageIds = ref<number[]>([]);

  /** Whether the active list has a further page (drives infinite scroll). */
  const hasMore = computed(() => {
    if (deps.activeView.value === "mailbox") return messagesCursor.value !== null;
    if (deps.activeView.value === "unified") return unifiedCursor.value !== null;
    return false; // The tag-filter view is a single non-paginated snapshot.
  });

  /** Records the mailbox list's next-page cursor after a fresh first-page load. */
  function setMailboxCursor(cursor: string | null): void {
    messagesCursor.value = cursor;
  }

  /** Records the unified list's next-page cursor after a fresh first-page load. */
  function setUnifiedCursor(cursor: string | null): void {
    unifiedCursor.value = cursor;
  }

  /**
   * Appends the next mailbox page, advancing the cursor. Guarded so a
   * mailbox/view switch mid-flight never appends stale rows, and so only one
   * fetch runs at a time.
   */
  async function loadMoreMessages(): Promise<void> {
    const mailboxId = deps.selectedMailboxId.value;
    const cursor = messagesCursor.value;
    if (mailboxId === null || cursor === null || loadingMore.value) return;
    loadingMore.value = true;
    try {
      const page = await listMessages(mailboxId, cursor, PAGE_SIZE);
      if (
        deps.selectedMailboxId.value === mailboxId &&
        deps.activeView.value === "mailbox" &&
        page !== null
      ) {
        deps.messages.value = [...deps.messages.value, ...page.items];
        messagesCursor.value = page.nextCursor;
      }
    } finally {
      loadingMore.value = false;
    }
  }

  /** Appends the next unified-inbox page, advancing the cursor. */
  async function loadMoreUnified(): Promise<void> {
    const cursor = unifiedCursor.value;
    if (cursor === null || loadingMore.value) return;
    loadingMore.value = true;
    try {
      const page = await listUnifiedInbox(cursor, PAGE_SIZE);
      if (deps.activeView.value === "unified" && page !== null) {
        deps.unifiedItems.value = [...deps.unifiedItems.value, ...page.items];
        unifiedCursor.value = page.nextCursor;
      }
    } finally {
      loadingMore.value = false;
    }
  }

  /** Dispatches an infinite-scroll request to the active list's loader. */
  function onLoadMore(): void {
    if (deps.activeView.value === "mailbox") void loadMoreMessages();
    else if (deps.activeView.value === "unified") void loadMoreUnified();
  }

  /** Records the list's current multi-selection (message ids) for track 3.2. */
  function onSelectionChange(ids: number[]): void {
    selectedMessageIds.value = ids;
  }

  /** Persists the threaded/flat toggle, reverting the optimistic flip on error. */
  async function onToggleThreaded(next: boolean): Promise<void> {
    threaded.value = next;
    try {
      await setListThreaded(next);
    } catch {
      threaded.value = !next;
    }
  }

  /** Loads the persisted threaded/flat preference (flat in the browser preview). */
  async function loadThreaded(): Promise<void> {
    const value = await getListThreaded();
    if (value !== null) threaded.value = value;
  }

  onMounted(loadThreaded);

  return {
    messagesCursor,
    unifiedCursor,
    loadingMore,
    threaded,
    selectedMessageIds,
    hasMore,
    setMailboxCursor,
    setUnifiedCursor,
    onLoadMore,
    onSelectionChange,
    onToggleThreaded,
  };
}
