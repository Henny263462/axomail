import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App from "../src/App.vue";
import { listAccounts } from "../src/api/accounts";
import { listDrafts } from "../src/api/compose";
import DraftListPane from "../src/components/list/DraftListPane.vue";
import type { AccountSummary, DraftDto } from "../src/types/mail";

vi.mock("../src/api/accounts", () => ({
  listAccounts: vi.fn(),
  createAccount: vi.fn(),
  deleteAccount: vi.fn(),
}));
vi.mock("../src/api/messages", () => ({
  listMessages: vi.fn(),
  markMessageSeen: vi.fn(),
  getMessageDetail: vi.fn(),
}));
vi.mock("../src/api/sync", () => ({
  syncAccount: vi.fn(),
  subscribeSyncEvents: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("../src/api/compose", () => ({
  sendMessage: vi.fn(),
  saveDraft: vi.fn(),
  listDrafts: vi.fn(),
  deleteDraft: vi.fn(),
  listOutbox: vi.fn(),
  retryOutbox: vi.fn(),
}));

const listAccountsMock = vi.mocked(listAccounts);
const listDraftsMock = vi.mocked(listDrafts);

const account: AccountSummary = {
  id: 1,
  displayName: "Ferber Mail",
  email: "klaus@theferbers.eu",
  authMechanism: "password",
  mailboxes: [{ id: 10, name: "Inbox", role: "inbox", unreadCount: 0 }],
};

const draft: DraftDto = {
  id: 88,
  accountId: 1,
  to: ["draft@example.org"],
  cc: [],
  bcc: [],
  subject: "Saved earlier",
  textBody: "Half-written",
  updatedUnix: 1_752_790_000,
};

describe("DraftListPane", () => {
  it("renders drafts and emits the selected id", async () => {
    const wrapper = mount(DraftListPane, {
      props: { drafts: [draft], selectedDraftId: null },
    });
    expect(wrapper.text()).toContain("Saved earlier");
    expect(wrapper.text()).toContain("draft@example.org");

    await wrapper.get("button.item").trigger("click");
    expect(wrapper.emitted("select")?.[0]).toEqual([88]);
  });

  it("shows an empty state when there are no drafts", () => {
    const wrapper = mount(DraftListPane, {
      props: { drafts: [], selectedDraftId: null },
    });
    expect(wrapper.text()).toContain("No drafts");
  });
});

describe("App drafts wiring", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listAccountsMock.mockResolvedValue([account]);
    listDraftsMock.mockResolvedValue([draft]);
  });

  it("opens the composer prefilled when a draft is selected", async () => {
    const wrapper = mount(App);
    await flushPromises();

    const draftsButton = wrapper
      .findAll("button.special")
      .find((candidate) => candidate.text() === "Drafts");
    if (draftsButton === undefined) throw new Error("drafts entry missing");
    await draftsButton.trigger("click");
    await flushPromises();

    expect(listDraftsMock).toHaveBeenCalledTimes(1);
    await wrapper.get('[aria-label="Drafts"] button.item').trigger("click");
    await flushPromises();

    const dialog = wrapper.get('[role="dialog"]');
    expect(dialog.attributes("aria-labelledby")).toBe("composer-title");
    expect(
      (wrapper.get("#compose-to").element as HTMLInputElement).value,
    ).toBe("draft@example.org");
  });
});
