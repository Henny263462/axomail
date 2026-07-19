import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  cleanupOfflineCache,
  downloadMailboxNow,
  getMailboxOfflinePolicy,
  getWorkOffline,
  setMailboxOfflinePolicy,
  setWorkOffline,
} from "../src/api/offline";
import FolderPropertiesDialog from "../src/components/offline/FolderPropertiesDialog.vue";
import StatusBar from "../src/components/layout/StatusBar.vue";
import MailListRegion from "../src/components/list/MailListRegion.vue";
import OutboxListPane from "../src/components/list/OutboxListPane.vue";
import { useWorkOffline } from "../src/composables/useWorkOffline";
import type { AccountSummary, OutboxItemDto } from "../src/types/mail";

// Pretend we run inside the desktop shell so the Work Offline toggle shows.
vi.mock("../src/lib/tauri", () => ({
  isTauri: () => true,
  safeInvoke: vi.fn().mockResolvedValue(null),
  invokeOrThrow: vi.fn().mockResolvedValue(null),
}));

vi.mock("../src/api/offline", () => ({
  getWorkOffline: vi.fn(),
  setWorkOffline: vi.fn(),
  getMailboxOfflinePolicy: vi.fn(),
  setMailboxOfflinePolicy: vi.fn(),
  downloadMailboxNow: vi.fn(),
  cleanupOfflineCache: vi.fn(),
}));

const getWorkOfflineMock = vi.mocked(getWorkOffline);
const setWorkOfflineMock = vi.mocked(setWorkOffline);
const getPolicyMock = vi.mocked(getMailboxOfflinePolicy);
const setPolicyMock = vi.mocked(setMailboxOfflinePolicy);
const downloadNowMock = vi.mocked(downloadMailboxNow);
const cleanupMock = vi.mocked(cleanupOfflineCache);

beforeEach(async () => {
  vi.clearAllMocks();
  getWorkOfflineMock.mockResolvedValue(false);
  setWorkOfflineMock.mockResolvedValue(null);
  getPolicyMock.mockResolvedValue({
    mailboxId: 10,
    mode: "recent",
    recentDays: 30,
  });
  setPolicyMock.mockResolvedValue(null);
  // Reset the shared singleton to a known online state between tests.
  await useWorkOffline().setOffline(false);
});

describe("Work Offline status-bar toggle", () => {
  it("loads the persisted state and toggles it through the command", async () => {
    getWorkOfflineMock.mockResolvedValue(false);
    const wrapper = mount(StatusBar);
    await flushPromises();

    const toggle = wrapper.get("button.offline-toggle");
    expect(toggle.text()).toBe("Online");
    expect(toggle.attributes("aria-pressed")).toBe("false");

    await toggle.trigger("click");
    await flushPromises();

    expect(setWorkOfflineMock).toHaveBeenCalledWith(true);
    expect(toggle.text()).toBe("Offline");
    expect(toggle.attributes("aria-pressed")).toBe("true");
  });

  it("reflects a persisted offline state on load", async () => {
    getWorkOfflineMock.mockResolvedValue(true);
    const wrapper = mount(StatusBar);
    await flushPromises();
    expect(wrapper.get("button.offline-toggle").text()).toBe("Offline");
  });
});

describe("Outbox offline labelling", () => {
  const pending: OutboxItemDto = {
    id: 1,
    accountId: 1,
    subject: "Draft reply",
    toSummary: "ops@example.org",
    state: "pending",
    attempts: 0,
    lastError: null,
    updatedUnix: 0,
  };

  it("labels an eligible queued row as offline while offline", async () => {
    await useWorkOffline().setOffline(true);
    const wrapper = mount(OutboxListPane, { props: { items: [pending] } });
    expect(wrapper.get(".badge").text()).toBe("Queued (offline)");
  });

  it("labels it as pending while online", () => {
    const wrapper = mount(OutboxListPane, { props: { items: [pending] } });
    expect(wrapper.get(".badge").text()).toBe("Pending");
  });
});

describe("FolderPropertiesDialog", () => {
  beforeEach(() => {
    getPolicyMock.mockResolvedValue({
      mailboxId: 10,
      mode: "recent",
      recentDays: 30,
    });
    setPolicyMock.mockResolvedValue(null);
    downloadNowMock.mockResolvedValue(null);
    cleanupMock.mockResolvedValue(3);
  });

  it("loads the policy and saves an edited mode", async () => {
    const wrapper = mount(FolderPropertiesDialog, {
      props: { mailboxId: 10, mailboxName: "Inbox" },
    });
    await flushPromises();

    expect(getPolicyMock).toHaveBeenCalledWith(10);
    expect(wrapper.text()).toContain("Inbox");
    expect(
      (wrapper.get('input[value="recent"]').element as HTMLInputElement)
        .checked,
    ).toBe(true);

    await wrapper.get('input[value="all"]').setValue();
    await wrapper.get("button.primary").trigger("click");
    await flushPromises();

    expect(setPolicyMock).toHaveBeenCalledWith(10, "all", 30);
    expect(wrapper.text()).toContain("Offline policy saved.");
  });

  it("starts an immediate download and cleans up the cache", async () => {
    const wrapper = mount(FolderPropertiesDialog, {
      props: { mailboxId: 10 },
    });
    await flushPromises();

    const buttons = wrapper.findAll("button");
    const download = buttons.find((b) => b.text() === "Download now");
    const cleanup = buttons.find((b) => b.text() === "Clean up cache");
    expect(download).toBeDefined();

    await download?.trigger("click");
    await flushPromises();
    expect(downloadNowMock).toHaveBeenCalledWith(10);
    expect(wrapper.text()).toContain("Download started");

    await cleanup?.trigger("click");
    await flushPromises();
    expect(cleanupMock).toHaveBeenCalledWith(10);
    expect(wrapper.text()).toContain("Removed 3 cached messages");
  });

  it("emits close from the Close button", async () => {
    const wrapper = mount(FolderPropertiesDialog, {
      props: { mailboxId: 10 },
    });
    await flushPromises();
    const close = wrapper.findAll("button").find((b) => b.text() === "Close");
    await close?.trigger("click");
    expect(wrapper.emitted("close")).toHaveLength(1);
  });
});

describe("MailListRegion folder-properties affordance", () => {
  const accounts: AccountSummary[] = [
    {
      id: 1,
      displayName: "Ferber Mail",
      email: "klaus@theferbers.eu",
      authMechanism: "password",
      mailboxes: [{ id: 10, name: "Inbox", role: "inbox", unreadCount: 0 }],
    },
  ];

  function mountRegion(view: "mailbox" | "unified", mailboxId: number | null) {
    return mount(MailListRegion, {
      props: {
        view,
        messages: [],
        unifiedItems: [],
        accounts,
        selectedMessageId: null,
        selectedMailboxId: mailboxId,
      },
    });
  }

  it("shows the Offline… button only for a single mailbox", () => {
    const mailbox = mountRegion("mailbox", 10);
    expect(
      mailbox.find('button[aria-label="Folder offline availability"]').exists(),
    ).toBe(true);

    const unified = mountRegion("unified", null);
    expect(
      unified.find('button[aria-label="Folder offline availability"]').exists(),
    ).toBe(false);
  });

  it("opens the folder-properties dialog for the shown mailbox", async () => {
    const wrapper = mountRegion("mailbox", 10);
    await wrapper
      .get('button[aria-label="Folder offline availability"]')
      .trigger("click");
    await flushPromises();
    // The dialog teleports to the document body.
    const dialog = document.body.querySelector('[role="dialog"]');
    expect(dialog).not.toBeNull();
    expect(dialog?.textContent).toContain("Offline availability");
    expect(getPolicyMock).toHaveBeenCalledWith(10);
    wrapper.unmount();
  });
});
