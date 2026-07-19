import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  createFolder,
  deleteFolder,
  refreshFolders,
  renameFolder,
  setFolderSubscribed,
} from "../src/api/folders";
import FolderTree from "../src/components/sidebar/FolderTree.vue";
import {
  buildFolderTree,
  leafName,
} from "../src/components/sidebar/folderTree";
import type { AccountSummary, MailboxSummary } from "../src/types/mail";

vi.mock("../src/api/folders", () => ({
  createFolder: vi.fn(),
  renameFolder: vi.fn(),
  deleteFolder: vi.fn(),
  setFolderSubscribed: vi.fn(),
  refreshFolders: vi.fn(),
}));

const createFolderMock = vi.mocked(createFolder);
const renameFolderMock = vi.mocked(renameFolder);
const deleteFolderMock = vi.mocked(deleteFolder);
const setFolderSubscribedMock = vi.mocked(setFolderSubscribed);
const refreshFoldersMock = vi.mocked(refreshFolders);

function mailbox(overrides: Partial<MailboxSummary> & { id: number }): MailboxSummary {
  return {
    name: "Folder",
    role: "custom",
    unreadCount: 0,
    totalCount: 0,
    parentId: null,
    delimiter: "/",
    subscribed: true,
    orphaned: false,
    ...overrides,
  };
}

const account: AccountSummary = {
  id: 1,
  displayName: "Work",
  email: "klaus@theferbers.eu",
  authMechanism: "password",
  mailboxes: [
    mailbox({ id: 1, name: "INBOX", role: "inbox", unreadCount: 2, totalCount: 5 }),
    mailbox({ id: 2, name: "INBOX/Work", parentId: 1, totalCount: 3 }),
    mailbox({ id: 3, name: "INBOX/Work/2026", parentId: 2, totalCount: 0 }),
  ],
};

function mountTree() {
  return mount(FolderTree, {
    props: { account, selectedMailboxId: null, activeView: "mailbox" as const },
  });
}

/** Opens the context menu for the folder whose row shows `leaf`. */
async function openMenuFor(
  wrapper: ReturnType<typeof mountTree>,
  leaf: string,
): Promise<void> {
  const row = wrapper
    .findAll(".folder-row")
    .find((candidate) => candidate.find(".folder-name").text() === leaf);
  if (row === undefined) throw new Error(`no folder row for ${leaf}`);
  await row.get(".folder-menu").trigger("click");
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("buildFolderTree", () => {
  it("nests by parentId and derives leaf + depth", () => {
    const roots = buildFolderTree(account.mailboxes);
    expect(roots).toHaveLength(1);
    expect(roots[0]?.leaf).toBe("INBOX");
    expect(roots[0]?.depth).toBe(0);
    const work = roots[0]?.children[0];
    expect(work?.leaf).toBe("Work");
    expect(work?.depth).toBe(1);
    expect(work?.children[0]?.leaf).toBe("2026");
    expect(work?.children[0]?.depth).toBe(2);
  });

  it("derives a leaf from the delimiter, falling back to the name", () => {
    expect(leafName(mailbox({ id: 9, name: "a/b/c", delimiter: "/" }))).toBe("c");
    expect(leafName(mailbox({ id: 9, name: "Flat", delimiter: null }))).toBe("Flat");
  });
});

describe("FolderTree", () => {
  it("renders the nested tree and emits selection", async () => {
    const wrapper = mountTree();
    expect(wrapper.text()).toContain("INBOX");
    expect(wrapper.text()).toContain("Work");
    expect(wrapper.text()).toContain("2026");

    const work = wrapper
      .findAll(".folder-button")
      .find((button) => button.find(".folder-name").text() === "Work");
    await work?.trigger("click");
    expect(wrapper.emitted("selectMailbox")?.[0]).toEqual([2]);
  });

  it("creates a subfolder through the context menu", async () => {
    createFolderMock.mockResolvedValue(mailbox({ id: 4, name: "INBOX/Work/New" }));
    const wrapper = mountTree();
    await openMenuFor(wrapper, "Work");

    const newItem = wrapper
      .findAll(".menu button")
      .find((button) => button.text() === "New subfolder");
    await newItem?.trigger("click");

    await wrapper.get("#folder-dialog-input").setValue("Reports");
    const create = wrapper
      .findAll(".dialog .button")
      .find((button) => button.text() === "Create");
    await create?.trigger("click");

    expect(createFolderMock).toHaveBeenCalledWith({
      accountId: 1,
      parentId: 2,
      name: "Reports",
    });
    expect(wrapper.emitted("changed")).toBeTruthy();
  });

  it("renames a folder, prefilling the current leaf", async () => {
    renameFolderMock.mockResolvedValue(null);
    const wrapper = mountTree();
    await openMenuFor(wrapper, "Work");

    const renameItem = wrapper
      .findAll(".menu button")
      .find((button) => button.text() === "Rename…");
    await renameItem?.trigger("click");

    const input = wrapper.get("#folder-dialog-input");
    expect((input.element as HTMLInputElement).value).toBe("Work");
    await input.setValue("Projects");
    const rename = wrapper
      .findAll(".dialog .button")
      .find((button) => button.text() === "Rename");
    await rename?.trigger("click");

    expect(renameFolderMock).toHaveBeenCalledWith(2, "Projects");
  });

  it("gates deletion of a non-empty folder behind a typed confirmation", async () => {
    deleteFolderMock.mockResolvedValue(null);
    const wrapper = mountTree();
    await openMenuFor(wrapper, "Work"); // Work has totalCount 3

    const del = wrapper
      .findAll(".menu button")
      .find((button) => button.text() === "Delete…");
    await del?.trigger("click");

    const confirm = () =>
      wrapper.findAll(".dialog .button").find((b) => b.text() === "Delete");
    expect(confirm()?.attributes("disabled")).toBeDefined();

    await wrapper.get("#folder-dialog-input").setValue("Work");
    expect(confirm()?.attributes("disabled")).toBeUndefined();
    await confirm()?.trigger("click");
    expect(deleteFolderMock).toHaveBeenCalledWith(2);
  });

  it("deletes an empty folder without a typed confirmation", async () => {
    deleteFolderMock.mockResolvedValue(null);
    const wrapper = mountTree();
    await openMenuFor(wrapper, "2026"); // totalCount 0

    const del = wrapper
      .findAll(".menu button")
      .find((button) => button.text() === "Delete…");
    await del?.trigger("click");
    // No confirmation input is shown; Delete is immediately enabled.
    expect(wrapper.find("#folder-dialog-input").exists()).toBe(false);
    const confirm = wrapper
      .findAll(".dialog .button")
      .find((button) => button.text() === "Delete");
    expect(confirm?.attributes("disabled")).toBeUndefined();
    await confirm?.trigger("click");
    expect(deleteFolderMock).toHaveBeenCalledWith(3);
  });

  it("toggles subscription from the menu", async () => {
    setFolderSubscribedMock.mockResolvedValue(null);
    const wrapper = mountTree();
    await openMenuFor(wrapper, "Work");
    const toggle = wrapper
      .findAll(".menu button")
      .find((button) => button.text() === "Unsubscribe");
    await toggle?.trigger("click");
    expect(setFolderSubscribedMock).toHaveBeenCalledWith(2, false);
  });

  it("refreshes folders from the toolbar", async () => {
    refreshFoldersMock.mockResolvedValue(null);
    const wrapper = mountTree();
    const refresh = wrapper
      .findAll(".tool")
      .find((button) => button.text().includes("Refresh"));
    await refresh?.trigger("click");
    expect(refreshFoldersMock).toHaveBeenCalledWith(1);
  });
});
