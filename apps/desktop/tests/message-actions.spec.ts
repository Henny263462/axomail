import { flushPromises, mount } from "@vue/test-utils";
import { defineComponent, ref } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  getMessageDetail,
  markMailboxRead,
  markMessagesRead,
  markMessageSeen,
} from "../src/api/messages";
import MessageViewPane from "../src/components/message/MessageViewPane.vue";
import { useMessageActions } from "../src/composables/useMessageActions";
import type {
  AccountSummary,
  MessageDetailDto,
  MessageListItem as MessageListItemModel,
} from "../src/types/mail";

vi.mock("../src/api/messages", () => ({
  markMessageSeen: vi.fn(),
  setMessageFlagged: vi.fn(),
  markMailboxRead: vi.fn(),
  markMessagesRead: vi.fn(),
  getMessageDetail: vi.fn(),
  setRemoteContentAllowed: vi.fn(),
}));

const markMessageSeenMock = vi.mocked(markMessageSeen);
const markMailboxReadMock = vi.mocked(markMailboxRead);
const markMessagesReadMock = vi.mocked(markMessagesRead);
const getMessageDetailMock = vi.mocked(getMessageDetail);

function listItem(over: Partial<MessageListItemModel> = {}): MessageListItemModel {
  return {
    id: 1,
    mailboxId: 10,
    subject: "Subject",
    fromName: "Ada",
    fromAddress: "ada@example.org",
    dateUnix: 1_752_000_000,
    seen: false,
    flagged: false,
    preview: null,
    tags: [],
    ...over,
  };
}

function detail(over: Partial<MessageDetailDto> = {}): MessageDetailDto {
  return {
    id: 1,
    subject: "Subject",
    fromName: "Ada",
    fromAddress: "ada@example.org",
    to: [],
    cc: [],
    dateUnix: 1_752_000_000,
    flagged: false,
    textBody: "body",
    sanitizedHtml: null,
    remoteBlockedCount: 0,
    remoteAllowed: false,
    scamFlags: [],
    hadActiveContent: false,
    attachments: [],
    messageId: null,
    inReplyTo: [],
    references: [],
    ...over,
  };
}

/** Mounts a throwaway component so the composable has a component context. */
function useActions(deps: Parameters<typeof useMessageActions>[0]) {
  let api!: ReturnType<typeof useMessageActions>;
  const wrapper = mount(
    defineComponent({
      setup() {
        api = useMessageActions(deps);
        return () => null;
      },
    }),
  );
  return { api, wrapper };
}

/** An account owning the two mailboxes used by the mark-read tests. */
function workAccount(unread: [number, number] = [1, 0]): AccountSummary {
  return {
    id: 1,
    displayName: "Work",
    email: "work@example.org",
    authMechanism: "password",
    mailboxes: [
      { id: 10, name: "Inbox", role: "inbox", unreadCount: unread[0] },
      { id: 11, name: "Sent", role: "sent", unreadCount: unread[1] },
    ],
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  markMessageSeenMock.mockResolvedValue(null);
  markMailboxReadMock.mockResolvedValue(0);
  markMessagesReadMock.mockResolvedValue(null);
});

describe("useMessageActions auto-read reactivity", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("marks the still-current row after the list reloads mid-delay", () => {
    const stale = listItem({ id: 1, mailboxId: 10, seen: false });
    const messages = ref([stale]);
    const { api, wrapper } = useActions({
      messages,
      unifiedItems: ref([]),
      accounts: ref([]),
      selectedMessageId: ref(1),
    });
    api.configureAutoRead(true, 500);

    api.scheduleAutoRead(1);
    // The list reloads: message 1 is now a fresh object; the captured one is
    // detached. The timer must mark the current row, never the stale object.
    const current = listItem({ id: 1, mailboxId: 10, seen: false });
    messages.value = [current];
    vi.advanceTimersByTime(500);

    expect(current.seen).toBe(true);
    expect(stale.seen).toBe(false);
    expect(markMessageSeenMock).toHaveBeenCalledWith(1, true);
    wrapper.unmount();
  });

  it("does not auto-mark before the persisted setting is applied", () => {
    const item = listItem({ id: 1, seen: false });
    const { api, wrapper } = useActions({
      messages: ref([item]),
      unifiedItems: ref([]),
      accounts: ref([]),
      selectedMessageId: ref(1),
    });

    // Settings have not resolved yet: nothing is marked, even though the
    // in-memory default is "enabled".
    api.scheduleAutoRead(1);
    expect(item.seen).toBe(false);
    expect(markMessageSeenMock).not.toHaveBeenCalled();

    // Once the persisted (enabled) setting arrives, a selection marks it.
    api.configureAutoRead(true, 0);
    api.scheduleAutoRead(1);
    expect(item.seen).toBe(true);
    wrapper.unmount();
  });

  it("applies a mid-session auto-read change on the next selection", () => {
    const item = listItem({ id: 1, seen: false });
    const { api, wrapper } = useActions({
      messages: ref([item]),
      unifiedItems: ref([]),
      accounts: ref([]),
      selectedMessageId: ref(1),
    });
    api.configureAutoRead(false, 0);
    api.scheduleAutoRead(1);
    expect(item.seen).toBe(false);

    // The user enables auto-read in Settings; the next open takes effect.
    api.configureAutoRead(true, 0);
    api.scheduleAutoRead(1);
    expect(item.seen).toBe(true);
    wrapper.unmount();
  });
});

describe("useMessageActions mark-all-read revert", () => {
  it("reverts the optimistic mark-all when the command fails", async () => {
    // safeInvoke returns null on failure; the optimistic batch must undo.
    markMailboxReadMock.mockResolvedValue(null);
    const rows = [
      listItem({ id: 1, mailboxId: 10, seen: false }),
      listItem({ id: 2, mailboxId: 10, seen: false }),
    ];
    const account = workAccount([2, 0]);
    const { api, wrapper } = useActions({
      messages: ref(rows),
      unifiedItems: ref([]),
      accounts: ref([account]),
      selectedMessageId: ref(null),
    });

    await api.markAllRead(10);
    expect(rows.every((m) => !m.seen)).toBe(true);
    expect(account.mailboxes[0]!.unreadCount).toBe(2);
    wrapper.unmount();
  });
});

describe("useMessageActions mark-list-read (tag view)", () => {
  function tagRows(): MessageListItemModel[] {
    return [
      listItem({ id: 1, mailboxId: 10, seen: false }),
      listItem({ id: 2, mailboxId: 11, seen: true }),
    ];
  }

  it("marks every unread row in the list and calls the set command", async () => {
    const rows = tagRows();
    const account = workAccount([1, 0]);
    const { api, wrapper } = useActions({
      messages: ref(rows),
      unifiedItems: ref([]),
      accounts: ref([account]),
      selectedMessageId: ref(null),
    });

    await api.markListRead();
    expect(rows[0]!.seen).toBe(true);
    // Only the unread row is sent; the already-read one is left out.
    expect(markMessagesReadMock).toHaveBeenCalledWith([1], true);
    expect(account.mailboxes[0]!.unreadCount).toBe(0);
    wrapper.unmount();
  });

  it("reverts the whole batch when the command rejects", async () => {
    markMessagesReadMock.mockRejectedValueOnce(new Error("offline"));
    const rows = tagRows();
    const account = workAccount([1, 0]);
    const { api, wrapper } = useActions({
      messages: ref(rows),
      unifiedItems: ref([]),
      accounts: ref([account]),
      selectedMessageId: ref(null),
    });

    await api.markListRead();
    await flushPromises();
    expect(rows[0]!.seen).toBe(false);
    expect(account.mailboxes[0]!.unreadCount).toBe(1);
    wrapper.unmount();
  });
});

describe("useMessageActions toggleSeen (the `u` shortcut)", () => {
  it("flips an unread message to read and decrements the badge", async () => {
    const row = listItem({ id: 1, mailboxId: 10, seen: false });
    const account = workAccount([1, 0]);
    const { api, wrapper } = useActions({
      messages: ref([row]),
      unifiedItems: ref([]),
      accounts: ref([account]),
      selectedMessageId: ref(1),
    });

    await api.toggleSeen(1);
    expect(row.seen).toBe(true);
    expect(markMessagesReadMock).toHaveBeenCalledWith([1], true);
    expect(account.mailboxes[0]!.unreadCount).toBe(0);
    wrapper.unmount();
  });

  it("flips a read message back to unread and increments the badge", async () => {
    const row = listItem({ id: 1, mailboxId: 10, seen: true });
    const account = workAccount([0, 0]);
    const { api, wrapper } = useActions({
      messages: ref([row]),
      unifiedItems: ref([]),
      accounts: ref([account]),
      selectedMessageId: ref(1),
    });

    await api.toggleSeen(1);
    expect(row.seen).toBe(false);
    expect(markMessagesReadMock).toHaveBeenCalledWith([1], false);
    expect(account.mailboxes[0]!.unreadCount).toBe(1);
    wrapper.unmount();
  });

  it("reverts the flip (row and badge) when the command rejects", async () => {
    markMessagesReadMock.mockRejectedValueOnce(new Error("offline"));
    const row = listItem({ id: 1, mailboxId: 10, seen: false });
    const account = workAccount([1, 0]);
    const { api, wrapper } = useActions({
      messages: ref([row]),
      unifiedItems: ref([]),
      accounts: ref([account]),
      selectedMessageId: ref(1),
    });

    await api.toggleSeen(1);
    await flushPromises();
    expect(row.seen).toBe(false);
    expect(account.mailboxes[0]!.unreadCount).toBe(1);
    wrapper.unmount();
  });
});

describe("MessageViewPane offline placeholder", () => {
  it("renders an honest empty state when the body is unavailable offline", async () => {
    getMessageDetailMock.mockResolvedValue(
      detail({ id: 5, offlineUnavailable: true, textBody: null }),
    );
    const wrapper = mount(MessageViewPane, { props: { messageId: 5 } });
    await flushPromises();

    expect(wrapper.text()).toContain("Not available offline");
    // The reading pane (and its star action) is not shown.
    expect(wrapper.find("button.star").exists()).toBe(false);
    wrapper.unmount();
  });
});
