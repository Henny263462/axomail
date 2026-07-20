import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ref } from "vue";

import {
  getJunkSettings,
  markJunk,
  markNotJunk,
  runJunkControls,
  setJunkSettings,
} from "../src/api/junk";
import JunkSection from "../src/components/settings/JunkSection.vue";
import type { SettingsSectionContext } from "../src/components/settings/context";
import { useMailboxActions } from "../src/composables/useMailboxActions";
import type {
  AccountSummary,
  MessageListItem,
  UnifiedItemDto,
} from "../src/types/mail";

// useMailboxActions imports these three api modules; stub all of them.
vi.mock("../src/api/junk", () => ({
  markJunk: vi.fn(),
  markNotJunk: vi.fn(),
  runJunkControls: vi.fn(),
  getJunkSettings: vi.fn(),
  setJunkSettings: vi.fn(),
}));
vi.mock("../src/api/actions", () => ({
  archiveMessages: vi.fn(),
  copyMessages: vi.fn(),
  deleteToTrash: vi.fn(),
  moveMessages: vi.fn(),
  undoMove: vi.fn(),
}));
vi.mock("../src/api/messages", () => ({
  markMessagesRead: vi.fn(),
}));

const markJunkMock = vi.mocked(markJunk);
const markNotJunkMock = vi.mocked(markNotJunk);
const runJunkControlsMock = vi.mocked(runJunkControls);
const getJunkSettingsMock = vi.mocked(getJunkSettings);
const setJunkSettingsMock = vi.mocked(setJunkSettings);

interface TauriGlobals {
  __TAURI_INTERNALS__?: unknown;
}

function row(id: number, mailboxId = 10): MessageListItem {
  return {
    id,
    mailboxId,
    subject: `m${id}`,
    fromName: "Ada",
    fromAddress: "ada@example.org",
    dateUnix: 1,
    seen: true,
    flagged: false,
    preview: "",
    tags: [],
  };
}

function makeActions() {
  const messages = ref<MessageListItem[]>([row(1), row(2)]);
  const unifiedItems = ref<UnifiedItemDto[]>([]);
  const accounts = ref<AccountSummary[]>([
    {
      id: 1,
      displayName: "Work",
      email: "k@example.org",
      authMechanism: "password",
      mailboxes: [{ id: 10, name: "Inbox", role: "inbox", unreadCount: 0 }],
    },
  ]);
  const selectedMessageId = ref<number | null>(1);
  const actions = useMailboxActions({
    messages,
    unifiedItems,
    accounts,
    selectedMessageId,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    dialogs: { open: vi.fn() } as any,
    reload: vi.fn(),
    clearSelection: vi.fn(),
  });
  return { actions, messages, selectedMessageId };
}

beforeEach(() => {
  vi.clearAllMocks();
  (globalThis as TauriGlobals).__TAURI_INTERNALS__ = {};
});

afterEach(() => {
  delete (globalThis as TauriGlobals).__TAURI_INTERNALS__;
});

describe("useMailboxActions junk", () => {
  it("marks junk, splicing the row and confirming with a notice", async () => {
    markJunkMock.mockResolvedValue({
      moves: [{ messageId: 1, fromMailboxId: 10 }],
    });
    const { actions, messages, selectedMessageId } = makeActions();

    await actions.junk([1]);
    await flushPromises();

    expect(markJunkMock).toHaveBeenCalledWith([1]);
    expect(messages.value.map((m) => m.id)).toEqual([2]);
    expect(selectedMessageId.value).toBeNull();
    expect(actions.notice.value).toBe("Marked 1 as junk");
  });

  it("marks not-junk through the api", async () => {
    markNotJunkMock.mockResolvedValue({ moves: [] });
    const { actions } = makeActions();

    await actions.notJunk([2]);
    await flushPromises();

    expect(markNotJunkMock).toHaveBeenCalledWith([2]);
    expect(actions.notice.value).toBe("Marked 1 as not junk");
  });

  it("reverts the optimistic splice when the backend fails", async () => {
    markJunkMock.mockRejectedValue("nope");
    const { actions, messages } = makeActions();

    await actions.junk([1]);
    await flushPromises();

    expect(messages.value.map((m) => m.id)).toEqual([1, 2]);
    expect(actions.notice.value).toBe("nope");
  });

  it("routes the dispatch action for junk", async () => {
    markJunkMock.mockResolvedValue({ moves: [] });
    const { actions } = makeActions();
    actions.dispatch("junk", [1]);
    await flushPromises();
    expect(markJunkMock).toHaveBeenCalledWith([1]);
  });
});

/** A minimal settings context with one inbox-bearing account. */
function stubContext(
  overrides: Partial<SettingsSectionContext> = {},
): SettingsSectionContext {
  return {
    accounts: [
      {
        id: 1,
        displayName: "Work",
        email: "k@example.org",
        authMechanism: "password",
        mailboxes: [
          { id: 10, name: "Inbox", role: "inbox", unreadCount: 0 },
          { id: 11, name: "Archive", role: "archive", unreadCount: 0 },
        ],
      },
    ],
    initialPolicies: [],
    notificationsEnabled: false,
    syncIntervalMinutes: 5,
    theme: "system",
    autoMarkReadEnabled: true,
    autoMarkReadDelayMs: 0,
    setNotificationsEnabled: vi.fn(),
    setSyncIntervalMinutes: vi.fn(),
    setTheme: vi.fn(),
    setAutoMarkRead: vi.fn(),
    reportError: vi.fn(),
    reportStatus: vi.fn(),
    notifyAccountsChanged: vi.fn(),
    notifyTagsChanged: vi.fn(),
    ...overrides,
  };
}

describe("JunkSection", () => {
  it("loads settings and shows the threshold as a percentage", async () => {
    getJunkSettingsMock.mockResolvedValue({
      enabled: true,
      threshold: 0.8,
      trustServerHeaders: true,
      whitelistContacts: false,
      markReadOnJunk: false,
    });
    const wrapper = mount(JunkSection, { props: { context: stubContext() } });
    await flushPromises();

    expect(wrapper.text()).toContain("80%");
    expect(
      wrapper.get<HTMLInputElement>("#settings-junk-enabled").element.checked,
    ).toBe(true);
  });

  it("persists a toggle change through the api", async () => {
    getJunkSettingsMock.mockResolvedValue({
      enabled: false,
      threshold: 0.9,
      trustServerHeaders: true,
      whitelistContacts: true,
      markReadOnJunk: false,
    });
    setJunkSettingsMock.mockResolvedValue(null);
    const wrapper = mount(JunkSection, { props: { context: stubContext() } });
    await flushPromises();

    await wrapper.get("#settings-junk-enabled").setValue(true);
    await flushPromises();

    expect(setJunkSettingsMock).toHaveBeenCalledWith(
      expect.objectContaining({ enabled: true }),
    );
  });

  it("reverts the enable checkbox when persistence fails", async () => {
    getJunkSettingsMock.mockResolvedValue({
      enabled: false,
      threshold: 0.9,
      trustServerHeaders: true,
      whitelistContacts: true,
      markReadOnJunk: false,
    });
    // Capture the enabled flag at call time — the reactive settings object is
    // passed by reference and the revert mutates it back afterward.
    let sentEnabled: boolean | undefined;
    setJunkSettingsMock.mockImplementation((s) => {
      sentEnabled = s.enabled;
      return Promise.reject("persist boom");
    });
    const context = stubContext();
    const wrapper = mount(JunkSection, { props: { context } });
    await flushPromises();

    const box = wrapper.get<HTMLInputElement>("#settings-junk-enabled");
    expect(box.element.checked).toBe(false);

    await box.setValue(true);
    await flushPromises();

    // The optimistic value was submitted…
    expect(sentEnabled).toBe(true);
    // …but the write was rejected, so the setting reverts rather than lingering
    // as an un-persisted change that would ride along with the next edit.
    expect(box.element.checked).toBe(false);
    expect(context.reportError).toHaveBeenLastCalledWith("persist boom");
    wrapper.unmount();
  });

  it("runs junk controls over every inbox and reports the total", async () => {
    getJunkSettingsMock.mockResolvedValue({
      enabled: true,
      threshold: 0.9,
      trustServerHeaders: true,
      whitelistContacts: true,
      markReadOnJunk: false,
    });
    runJunkControlsMock.mockResolvedValue(3);
    const context = stubContext();
    const wrapper = mount(JunkSection, { props: { context } });
    await flushPromises();

    await wrapper.get(".run").trigger("click");
    await flushPromises();

    expect(runJunkControlsMock).toHaveBeenCalledWith(10);
    expect(runJunkControlsMock).toHaveBeenCalledTimes(1);
    expect(context.reportStatus).toHaveBeenLastCalledWith(
      "Filed 3 messages as junk.",
    );
  });
});
