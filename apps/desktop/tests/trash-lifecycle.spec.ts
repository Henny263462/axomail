import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { compactStorage, emptyTrash } from "../src/api/folders";
import FolderContextMenu from "../src/components/sidebar/FolderContextMenu.vue";
import FolderTree from "../src/components/sidebar/FolderTree.vue";
import type { AccountSummary, MailboxSummary } from "../src/types/mail";

// FolderTree imports the whole folder-api surface; the factory must define all
// of it (the spec only asserts on emptyTrash / compactStorage).
vi.mock("../src/api/folders", () => ({
  createFolder: vi.fn(),
  renameFolder: vi.fn(),
  deleteFolder: vi.fn(),
  setFolderSubscribed: vi.fn(),
  refreshFolders: vi.fn(),
  emptyTrash: vi.fn(),
  compactStorage: vi.fn(),
  getEmptyTrashOnExit: vi.fn(),
  setEmptyTrashOnExit: vi.fn(),
}));

const emptyTrashMock = vi.mocked(emptyTrash);
const compactMock = vi.mocked(compactStorage);

function mailbox(over: Partial<MailboxSummary> & { id: number }): MailboxSummary {
  return {
    name: "Folder",
    role: "custom",
    unreadCount: 0,
    totalCount: 0,
    parentId: null,
    delimiter: "/",
    subscribed: true,
    orphaned: false,
    ...over,
  };
}

const inboxFolder = mailbox({ id: 1, name: "INBOX", role: "inbox" });
const trashFolder = mailbox({ id: 2, name: "Trash", role: "trash", totalCount: 4 });
const account: AccountSummary = {
  id: 1,
  displayName: "Work",
  email: "klaus@theferbers.eu",
  authMechanism: "password",
  mailboxes: [inboxFolder, trashFolder],
};

beforeEach(() => vi.clearAllMocks());

describe("FolderContextMenu", () => {
  it("offers Empty Trash only on a Trash-role folder, Compact always", () => {
    const trash = mount(FolderContextMenu, {
      props: { folder: trashFolder, x: 0, y: 0 },
      attachTo: document.body,
    });
    const trashLabels = trash.findAll('[role="menuitem"]').map((b) => b.text());
    expect(trashLabels).toContain("Empty Trash…");
    expect(trashLabels).toContain("Compact");
    trash.unmount();

    const inbox = mount(FolderContextMenu, {
      props: { folder: inboxFolder, x: 0, y: 0 },
      attachTo: document.body,
    });
    const inboxLabels = inbox.findAll('[role="menuitem"]').map((b) => b.text());
    expect(inboxLabels).not.toContain("Empty Trash…");
    expect(inboxLabels).toContain("Compact");
    inbox.unmount();
  });

  it("emits emptyTrash and compact", async () => {
    const wrapper = mount(FolderContextMenu, {
      props: { folder: trashFolder, x: 0, y: 0 },
      attachTo: document.body,
    });
    const empty = wrapper
      .findAll('[role="menuitem"]')
      .find((b) => b.text() === "Empty Trash…");
    await empty?.trigger("click");
    expect(wrapper.emitted("emptyTrash")).toBeTruthy();
    wrapper.unmount();
  });
});

describe("FolderTree Empty Trash", () => {
  function mountTree() {
    return mount(FolderTree, {
      props: { account, selectedMailboxId: null, activeView: "mailbox" as const },
      attachTo: document.body,
    });
  }

  async function openTrashMenu(wrapper: ReturnType<typeof mountTree>): Promise<void> {
    const row = wrapper
      .findAll(".folder-row")
      .find((r) => r.find(".folder-name").text() === "Trash");
    if (row === undefined) throw new Error("no Trash row");
    await row.get(".folder-menu").trigger("click");
  }

  it("requires the typed word 'empty' before emptying, then calls the command", async () => {
    emptyTrashMock.mockResolvedValue(4);
    const wrapper = mountTree();
    await openTrashMenu(wrapper);

    const item = wrapper
      .findAll('[role="menuitem"]')
      .find((b) => b.text() === "Empty Trash…");
    await item?.trigger("click");

    // A confirm dialog demanding a typed word appears.
    const dialog = wrapper.get('[role="alertdialog"]');
    expect(dialog.text()).toContain("Empty Trash");
    expect(dialog.text().toLowerCase()).toContain("permanently");

    const confirm = dialog
      .findAll("button")
      .find((b) => b.text() === "Empty Trash");
    if (confirm === undefined) throw new Error("confirm button missing");
    // Disabled until the exact word is typed.
    expect(confirm.attributes("disabled")).toBeDefined();

    await dialog.get("input").setValue("empty");
    expect(confirm.attributes("disabled")).toBeUndefined();

    await confirm.trigger("click");
    await flushPromises();
    expect(emptyTrashMock).toHaveBeenCalledWith(2);
    wrapper.unmount();
  });

  it("compacts storage from the folder menu", async () => {
    compactMock.mockResolvedValue({ reclaimed: 2 });
    const wrapper = mountTree();
    await openTrashMenu(wrapper);
    const item = wrapper
      .findAll('[role="menuitem"]')
      .find((b) => b.text() === "Compact");
    await item?.trigger("click");
    await flushPromises();
    expect(compactMock).toHaveBeenCalled();
    wrapper.unmount();
  });
});
