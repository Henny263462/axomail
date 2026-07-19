import { flushPromises, mount } from "@vue/test-utils";
import { defineComponent, ref } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  getMessageDetail,
  markMailboxRead,
  markMessageSeen,
  setMessageFlagged,
} from "../src/api/messages";
import AutoReadSection from "../src/components/settings/AutoReadSection.vue";
import type { SettingsSectionContext } from "../src/components/settings/context";
import MessageRow from "../src/components/list/MessageRow.vue";
import MessageListPane from "../src/components/list/MessageListPane.vue";
import MessageViewPane from "../src/components/message/MessageViewPane.vue";
import { useMessageActions } from "../src/composables/useMessageActions";
import type {
  AccountSummary,
  MessageDetailDto,
  MessageListItem as MessageListItemModel,
} from "../src/types/mail";

vi.mock("../src/api/messages", () => ({
  listMessages: vi.fn(),
  markMessageSeen: vi.fn(),
  setMessageFlagged: vi.fn(),
  markMailboxRead: vi.fn(),
  getMessageDetail: vi.fn(),
  setRemoteContentAllowed: vi.fn(),
}));

const markMessageSeenMock = vi.mocked(markMessageSeen);
const setMessageFlaggedMock = vi.mocked(setMessageFlagged);
const markMailboxReadMock = vi.mocked(markMailboxRead);
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

beforeEach(() => {
  vi.clearAllMocks();
  markMessageSeenMock.mockResolvedValue(null);
  setMessageFlaggedMock.mockResolvedValue(null);
  markMailboxReadMock.mockResolvedValue(0);
});

function rowProps(message: MessageListItemModel) {
  return {
    message,
    rowDomId: "row-1",
    checked: false,
    current: false,
    active: false,
  };
}

describe("MessageRow star", () => {
  it("reflects flagged as aria-pressed and emits toggleFlag on click", async () => {
    const wrapper = mount(MessageRow, {
      props: rowProps(listItem({ flagged: true })),
    });
    const star = wrapper.get("button.star");
    expect(star.attributes("aria-pressed")).toBe("true");
    expect(star.attributes("aria-label")).toBe("Remove star");

    await star.trigger("click");
    expect(wrapper.emitted("toggleFlag")?.[0]).toEqual([1]);
    // The star sits beside the row button, never nested inside it.
    expect(wrapper.findAll("button.item button").length).toBe(0);
  });

  it("labels an unstarred message for adding a star", () => {
    const wrapper = mount(MessageRow, {
      props: rowProps(listItem({ flagged: false })),
    });
    expect(wrapper.get("button.star").attributes("aria-label")).toBe("Add star");
  });
});

describe("MessageListPane mark all read", () => {
  it("enables the action only with unread messages and emits it", async () => {
    const wrapper = mount(MessageListPane, {
      props: {
        messages: [listItem({ id: 1, seen: false })],
        selectedMessageId: null,
        threaded: false,
      },
    });
    const button = wrapper.get("button.mark-read");
    expect(button.attributes("disabled")).toBeUndefined();
    await button.trigger("click");
    expect(wrapper.emitted("markAllRead")).toHaveLength(1);
  });

  it("disables the action when every message is read", () => {
    const wrapper = mount(MessageListPane, {
      props: {
        messages: [listItem({ id: 1, seen: true })],
        selectedMessageId: null,
        threaded: false,
      },
    });
    expect(wrapper.get("button.mark-read").attributes("disabled")).toBeDefined();
  });
});

describe("useMessageActions star toggle", () => {
  it("optimistically flips the star and persists it", async () => {
    const item = listItem({ id: 1, flagged: false });
    const { api, wrapper } = useActions({
      messages: ref([item]),
      unifiedItems: ref([]),
      accounts: ref([]),
      selectedMessageId: ref(1),
    });

    await api.toggleStar(1);
    expect(item.flagged).toBe(true);
    expect(setMessageFlaggedMock).toHaveBeenCalledWith(1, true);
    wrapper.unmount();
  });

  it("reverts the flip when the backend rejects", async () => {
    setMessageFlaggedMock.mockRejectedValueOnce(new Error("offline"));
    const item = listItem({ id: 1, flagged: false });
    const { api, wrapper } = useActions({
      messages: ref([item]),
      unifiedItems: ref([]),
      accounts: ref([]),
      selectedMessageId: ref(1),
    });

    await api.toggleStar(1);
    await flushPromises();
    expect(item.flagged).toBe(false);
    wrapper.unmount();
  });
});

describe("useMessageActions auto mark read", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("marks immediately when the delay is zero", () => {
    const item = listItem({ id: 1, seen: false });
    const { api, wrapper } = useActions({
      messages: ref([item]),
      unifiedItems: ref([]),
      accounts: ref<AccountSummary[]>([]),
      selectedMessageId: ref(1),
    });
    api.configureAutoRead(true, 0);

    api.scheduleAutoRead(1);
    expect(item.seen).toBe(true);
    expect(markMessageSeenMock).toHaveBeenCalledWith(1, true);
    wrapper.unmount();
  });

  it("waits for the configured delay before marking read", () => {
    const item = listItem({ id: 1, seen: false });
    const { api, wrapper } = useActions({
      messages: ref([item]),
      unifiedItems: ref([]),
      accounts: ref([]),
      selectedMessageId: ref(1),
    });
    api.configureAutoRead(true, 500);

    api.scheduleAutoRead(1);
    expect(item.seen).toBe(false);
    vi.advanceTimersByTime(500);
    expect(item.seen).toBe(true);
    expect(markMessageSeenMock).toHaveBeenCalledWith(1, true);
    wrapper.unmount();
  });

  it("does not mark read when a newer selection replaces it mid-delay", () => {
    const first = listItem({ id: 1, seen: false });
    const second = listItem({ id: 2, seen: false });
    const selectedMessageId = ref<number | null>(1);
    const { api, wrapper } = useActions({
      messages: ref([first, second]),
      unifiedItems: ref([]),
      accounts: ref([]),
      selectedMessageId,
    });
    api.configureAutoRead(true, 500);

    api.scheduleAutoRead(1);
    selectedMessageId.value = 2;
    api.scheduleAutoRead(2);
    vi.advanceTimersByTime(500);

    expect(first.seen).toBe(false);
    expect(second.seen).toBe(true);
    wrapper.unmount();
  });

  it("never marks read when auto-read is disabled", () => {
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
    expect(markMessageSeenMock).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});

describe("useMessageActions mark all read", () => {
  it("marks every list row read, zeroes the badge, and calls the command", async () => {
    const rows = [
      listItem({ id: 1, mailboxId: 10, seen: false }),
      listItem({ id: 2, mailboxId: 10, seen: false }),
    ];
    const mailbox = {
      id: 10,
      name: "Inbox",
      role: "inbox" as const,
      unreadCount: 2,
    };
    const account: AccountSummary = {
      id: 1,
      displayName: "Work",
      email: "work@example.org",
      authMechanism: "password",
      mailboxes: [mailbox],
    };
    const { api, wrapper } = useActions({
      messages: ref(rows),
      unifiedItems: ref([]),
      accounts: ref([account]),
      selectedMessageId: ref(null),
    });

    await api.markAllRead(10);
    expect(rows.every((m) => m.seen)).toBe(true);
    expect(mailbox.unreadCount).toBe(0);
    expect(markMailboxReadMock).toHaveBeenCalledWith(10);
    wrapper.unmount();
  });
});

describe("MessageViewPane reading-pane star", () => {
  it("toggles the star, persists it, and notifies the host", async () => {
    getMessageDetailMock.mockResolvedValue(detail({ id: 7, flagged: false }));
    const wrapper = mount(MessageViewPane, { props: { messageId: 7 } });
    await flushPromises();

    const star = wrapper.get("button.star");
    expect(star.attributes("aria-pressed")).toBe("false");
    await star.trigger("click");
    await flushPromises();

    expect(setMessageFlaggedMock).toHaveBeenCalledWith(7, true);
    expect(wrapper.emitted("flagChanged")?.[0]).toEqual([
      { messageId: 7, flagged: true },
    ]);
    expect(wrapper.get("button.star").attributes("aria-pressed")).toBe("true");
  });
});

describe("AutoReadSection", () => {
  function stub(
    over: Partial<SettingsSectionContext> = {},
  ): SettingsSectionContext {
    return {
      accounts: [],
      initialPolicies: [],
      notificationsEnabled: false,
      syncIntervalMinutes: 5,
      theme: "system",
      autoMarkReadEnabled: true,
      autoMarkReadDelayMs: 2000,
      setNotificationsEnabled: vi.fn(),
      setSyncIntervalMinutes: vi.fn(),
      setTheme: vi.fn(),
      setAutoMarkRead: vi.fn(),
      reportError: vi.fn(),
      reportStatus: vi.fn(),
      notifyAccountsChanged: vi.fn(),
      notifyTagsChanged: vi.fn(),
      ...over,
    };
  }

  it("shows the delay in seconds and stages a millisecond value", async () => {
    const context = stub();
    const wrapper = mount(AutoReadSection, { props: { context } });
    expect(
      (wrapper.get("#settings-auto-read-delay").element as HTMLInputElement)
        .value,
    ).toBe("2");

    await wrapper.get("#settings-auto-read-delay").setValue("5");
    expect(context.setAutoMarkRead).toHaveBeenCalledWith(true, 5000);
  });

  it("stages the enabled flag and disables the delay when off", async () => {
    const context = stub({ autoMarkReadEnabled: false });
    const wrapper = mount(AutoReadSection, { props: { context } });
    expect(
      wrapper.get("#settings-auto-read-delay").attributes("disabled"),
    ).toBeDefined();

    await wrapper.get("#settings-auto-read").setValue(true);
    expect(context.setAutoMarkRead).toHaveBeenCalledWith(true, 2000);
  });
});
