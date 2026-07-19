import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ref } from "vue";

import {
  archiveMessages,
  copyMessages,
  deleteToTrash,
  moveMessages,
  undoMove,
} from "../src/api/actions";
import UndoToast from "../src/components/common/UndoToast.vue";
import RowContextMenu from "../src/components/list/RowContextMenu.vue";
import MoveToFolderDialog from "../src/components/message/MoveToFolderDialog.vue";
import MessageViewPane from "../src/components/message/MessageViewPane.vue";
import { getMessageDetail } from "../src/api/messages";
import { useMailboxActions } from "../src/composables/useMailboxActions";
import type {
  AccountSummary,
  MessageDetailDto,
  MessageListItem,
} from "../src/types/mail";

vi.mock("../src/api/actions", () => ({
  archiveMessages: vi.fn(),
  copyMessages: vi.fn(),
  deleteToTrash: vi.fn(),
  moveMessages: vi.fn(),
  undoMove: vi.fn(),
}));
vi.mock("../src/api/messages", () => ({
  getMessageDetail: vi.fn(),
  setMessageFlagged: vi.fn(),
  setRemoteContentAllowed: vi.fn(),
  markMessagesRead: vi.fn(),
}));

const archiveMock = vi.mocked(archiveMessages);
const copyMock = vi.mocked(copyMessages);
const deleteMock = vi.mocked(deleteToTrash);
const moveMock = vi.mocked(moveMessages);
const undoMock = vi.mocked(undoMove);

function row(id: number, seen = true): MessageListItem {
  return {
    id,
    mailboxId: 10,
    subject: `m${id}`,
    fromName: "Ada",
    fromAddress: "ada@example.org",
    dateUnix: 1,
    seen,
    flagged: false,
    preview: "",
    tags: [],
  };
}

function makeActions() {
  const messages = ref<MessageListItem[]>([row(1, false), row(2)]);
  const accounts = ref<AccountSummary[]>([
    {
      id: 1,
      displayName: "Work",
      email: "k@example.org",
      authMechanism: "password",
      mailboxes: [
        { id: 10, name: "Inbox", role: "inbox", unreadCount: 1 },
        { id: 11, name: "Archive", role: "archive", unreadCount: 0 },
      ],
    },
  ]);
  const selectedMessageId = ref<number | null>(1);
  const dialogs = { open: vi.fn() };
  const reload = vi.fn();
  const actions = useMailboxActions({
    messages,
    unifiedItems: ref([]),
    accounts,
    selectedMessageId,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    dialogs: dialogs as any,
    reload,
  });
  return { actions, messages, accounts, selectedMessageId, dialogs, reload };
}

beforeEach(() => vi.clearAllMocks());

describe("useMailboxActions", () => {
  it("archive splices the row out, clears the open message, and raises undo", async () => {
    archiveMock.mockResolvedValue({ moves: [{ messageId: 1, fromMailboxId: 10 }] });
    const { actions, messages, accounts, selectedMessageId } = makeActions();

    await actions.archive([1]);

    // Optimistic removal + unread badge decrement for the unseen row.
    expect(messages.value.map((m) => m.id)).toEqual([2]);
    expect(accounts.value[0]?.mailboxes[0]?.unreadCount).toBe(0);
    // The open message (id 1) was relocated, so it is deselected.
    expect(selectedMessageId.value).toBeNull();
    // Undo toast records the inverse.
    expect(actions.undoState.value?.message).toBe("Archived 1");
    expect(actions.undoState.value?.moves).toEqual([
      { messageId: 1, fromMailboxId: 10 },
    ]);
  });

  it("undo moves the messages back and reloads the list", async () => {
    archiveMock.mockResolvedValue({ moves: [{ messageId: 1, fromMailboxId: 10 }] });
    undoMock.mockResolvedValue(null);
    const { actions, reload } = makeActions();
    await actions.archive([1]);

    await actions.undo();

    expect(undoMock).toHaveBeenCalledWith([{ messageId: 1, fromMailboxId: 10 }]);
    expect(reload).toHaveBeenCalled();
    expect(actions.undoState.value).toBeNull();
  });

  it("delete reverts (reloads) and notices on a backend failure", async () => {
    deleteMock.mockRejectedValue("No Trash folder");
    const { actions, reload } = makeActions();

    await actions.remove([2]);

    expect(reload).toHaveBeenCalled();
    expect(actions.notice.value).toBe("No Trash folder");
    expect(actions.undoState.value).toBeNull();
  });

  it("dispatch('move') opens the folder picker for the ids", () => {
    const { actions, dialogs } = makeActions();
    actions.dispatch("move", [1, 2]);
    expect(dialogs.open).toHaveBeenCalledWith("moveToFolder", {
      ids: [1, 2],
      mode: "move",
    });
  });

  it("copy leaves the list untouched and shows a notice", async () => {
    copyMock.mockResolvedValue(null);
    const { actions, messages } = makeActions();

    await actions.onFolderPicked({
      ids: [1],
      mode: "copy",
      mailboxId: 11,
      mailboxName: "Archive",
    });

    expect(copyMock).toHaveBeenCalledWith([1], 11);
    expect(messages.value.map((m) => m.id)).toEqual([1, 2]);
    expect(actions.notice.value).toBe("Copied to Archive");
    expect(moveMock).not.toHaveBeenCalled();
  });
});

describe("MoveToFolderDialog", () => {
  const accounts: AccountSummary[] = [
    {
      id: 1,
      displayName: "Work",
      email: "k@example.org",
      authMechanism: "password",
      mailboxes: [{ id: 11, name: "Archive", role: "archive", unreadCount: 0 }],
    },
  ];

  it("lists folders and emits the picked target with the pending ids", async () => {
    const wrapper = mount(MoveToFolderDialog, {
      props: { accounts, ids: [7], mode: "move" },
    });
    expect(wrapper.get('[role="dialog"]').text()).toContain("Move to folder");

    const folder = wrapper
      .findAll("button.folder")
      .find((b) => b.text() === "Archive");
    if (folder === undefined) throw new Error("folder button missing");
    await folder.trigger("click");

    expect(wrapper.emitted("picked")?.[0]?.[0]).toEqual({
      ids: [7],
      mode: "move",
      mailboxId: 11,
      mailboxName: "Archive",
    });
  });
});

describe("RowContextMenu", () => {
  it("offers the relocation actions and emits the chosen one", () => {
    // The menu renders through a Teleport, so query the document body.
    const wrapper = mount(RowContextMenu, { props: { x: 5, y: 6 } });
    const items = [...document.body.querySelectorAll('[role="menuitem"]')];
    const labels = items.map((i) => i.textContent?.trim());
    expect(labels).toEqual(["Archive", "Move to…", "Copy to…", "Mark read", "Delete"]);

    const del = items.find((i) => i.textContent?.trim() === "Delete");
    (del as HTMLElement).click();
    expect(wrapper.emitted("action")?.[0]).toEqual(["delete"]);
    expect(wrapper.emitted("close")).toBeTruthy();
    wrapper.unmount();
  });
});

describe("UndoToast", () => {
  it("shows the undo prompt and emits undo", () => {
    const wrapper = mount(UndoToast, {
      props: { undoMessage: "Deleted 3", notice: null },
    });
    expect(document.body.textContent).toContain("Deleted 3");
    const undo = [...document.body.querySelectorAll("button")].find(
      (b) => b.textContent?.trim() === "Undo",
    );
    (undo as HTMLElement).click();
    expect(wrapper.emitted("undo")).toBeTruthy();
    wrapper.unmount();
  });
});

describe("reading-pane action bar", () => {
  const detail: MessageDetailDto = {
    id: 501,
    subject: "Hello",
    fromName: "Ada",
    fromAddress: "ada@example.org",
    to: [],
    cc: [],
    dateUnix: 1,
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
  };

  it("emits move/copy/archive/delete for the open message", async () => {
    vi.mocked(getMessageDetail).mockResolvedValue(detail);
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    for (const [label, event] of [
      ["Archive", "archive"],
      ["Move to…", "move"],
      ["Copy to…", "copy"],
      ["Delete", "deleteMessage"],
    ] as const) {
      const button = wrapper
        .findAll("button.action")
        .find((b) => b.text() === label);
      if (button === undefined) throw new Error(`${label} button missing`);
      await button.trigger("click");
      expect(wrapper.emitted(event)?.at(-1)).toEqual([501]);
    }
  });
});
