import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope, nextTick, ref } from "vue";

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
  UnifiedItemDto,
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

/** The honest browser-preview / unreachable-backend notice. */
const UNAVAILABLE = "This action needs the desktop app.";

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

function unifiedRow(id: number, seen = true): UnifiedItemDto {
  return { ...row(id, seen), accountId: 1 };
}

function makeActions(over?: { unified?: UnifiedItemDto[]; inboxUnread?: number }) {
  const messages = ref<MessageListItem[]>([row(1, false), row(2)]);
  const unifiedItems = ref<UnifiedItemDto[]>(over?.unified ?? []);
  const accounts = ref<AccountSummary[]>([
    {
      id: 1,
      displayName: "Work",
      email: "k@example.org",
      authMechanism: "password",
      mailboxes: [
        { id: 10, name: "Inbox", role: "inbox", unreadCount: over?.inboxUnread ?? 1 },
        { id: 11, name: "Archive", role: "archive", unreadCount: 0 },
      ],
    },
  ]);
  const selectedMessageId = ref<number | null>(1);
  const dialogs = { open: vi.fn() };
  const reload = vi.fn();
  const clearSelection = vi.fn();
  const actions = useMailboxActions({
    messages,
    unifiedItems,
    accounts,
    selectedMessageId,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    dialogs: dialogs as any,
    reload,
    clearSelection,
  });
  return {
    actions,
    messages,
    unifiedItems,
    accounts,
    selectedMessageId,
    dialogs,
    reload,
    clearSelection,
  };
}

beforeEach(() => vi.clearAllMocks());

describe("useMailboxActions", () => {
  it("archive splices the row out, clears the open message, raises undo, and clears selection", async () => {
    archiveMock.mockResolvedValue({ moves: [{ messageId: 1, fromMailboxId: 10 }] });
    const { actions, messages, accounts, selectedMessageId, clearSelection } =
      makeActions();

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
    // A completed relocation clears the list's multi-selection (finding 2).
    expect(clearSelection).toHaveBeenCalledTimes(1);
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

  it("a rejected relocation fully reverts rows without touching the selection (finding 4)", async () => {
    deleteMock.mockRejectedValue("No Trash folder");
    const { actions, messages, clearSelection } = makeActions();

    await actions.remove([2]);

    // The optimistic removal is undone locally — no reappear-on-reload needed.
    expect(messages.value.map((m) => m.id)).toEqual([1, 2]);
    expect(actions.notice.value).toBe("No Trash folder");
    expect(actions.undoState.value).toBeNull();
    // A failed relocation keeps the selection so the user can retry (finding 2).
    expect(clearSelection).not.toHaveBeenCalled();
  });

  it("rejecting an archive of the open unread message restores detail AND badge (finding 4)", async () => {
    archiveMock.mockRejectedValue("Server said no");
    const { actions, messages, accounts, selectedMessageId } = makeActions();

    await actions.archive([1]);

    expect(messages.value.map((m) => m.id)).toEqual([1, 2]);
    // The open detail is restored (id 1 was the acted-on open message).
    expect(selectedMessageId.value).toBe(1);
    // The unread badge decrement is restored.
    expect(accounts.value[0]?.mailboxes[0]?.unreadCount).toBe(1);
    expect(actions.notice.value).toBe("Server said no");
  });

  it("treats a null relocation result as unavailable, reverting the optimistic removal (finding 3)", async () => {
    // safeInvoke/invokeOrThrow resolve null when the bridge is absent.
    archiveMock.mockResolvedValue(null);
    const { actions, messages, accounts, selectedMessageId, clearSelection } =
      makeActions();

    await actions.archive([1]);

    // Nothing happened server-side, so the removal is reverted, not confirmed.
    expect(messages.value.map((m) => m.id)).toEqual([1, 2]);
    expect(selectedMessageId.value).toBe(1);
    expect(accounts.value[0]?.mailboxes[0]?.unreadCount).toBe(1);
    expect(actions.notice.value).toBe(UNAVAILABLE);
    expect(actions.undoState.value).toBeNull();
    expect(clearSelection).not.toHaveBeenCalled();
  });

  it("decrements a folder badge exactly once for a message in both caches (finding 5)", async () => {
    archiveMock.mockResolvedValue({ moves: [{ messageId: 1, fromMailboxId: 10 }] });
    // id 1 is unseen and present in BOTH the mailbox and unified caches.
    const { actions, accounts } = makeActions({
      unified: [unifiedRow(1, false)],
      inboxUnread: 2,
    });

    await actions.archive([1]);

    // Exactly one decrement (2 -> 1), not two (2 -> 0).
    expect(accounts.value[0]?.mailboxes[0]?.unreadCount).toBe(1);
  });

  it("dispatch('move') opens the folder picker for the ids", () => {
    const { actions, dialogs } = makeActions();
    actions.dispatch("move", [1, 2]);
    expect(dialogs.open).toHaveBeenCalledWith("moveToFolder", {
      ids: [1, 2],
      mode: "move",
    });
  });

  it("copy in the browser preview reports unavailable, never a false success (finding 3)", async () => {
    const { actions, messages } = makeActions();

    await actions.onFolderPicked({
      ids: [1],
      mode: "copy",
      mailboxId: 11,
      mailboxName: "Archive",
    });

    expect(copyMock).not.toHaveBeenCalled();
    expect(messages.value.map((m) => m.id)).toEqual([1, 2]);
    expect(actions.notice.value).toBe(UNAVAILABLE);
    expect(moveMock).not.toHaveBeenCalled();
  });

  it("copy performs the copy and confirms it in the desktop app", async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (globalThis as any).__TAURI_INTERNALS__ = {};
    try {
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
    } finally {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      delete (globalThis as any).__TAURI_INTERNALS__;
    }
  });

  it("clears pending notice/undo timers on scope dispose (finding 8)", async () => {
    vi.useFakeTimers();
    try {
      const scope = effectScope();
      let actions!: ReturnType<typeof useMailboxActions>;
      scope.run(() => {
        actions = useMailboxActions({
          messages: ref<MessageListItem[]>([]),
          unifiedItems: ref<UnifiedItemDto[]>([]),
          accounts: ref<AccountSummary[]>([]),
          selectedMessageId: ref<number | null>(null),
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          dialogs: { open: vi.fn() } as any,
          reload: vi.fn(),
        });
      });

      // The browser-preview copy path sets a timed notice.
      await actions.copyTo([1], 11, "Archive");
      const pending = actions.notice.value;
      expect(pending).toBe(UNAVAILABLE);

      scope.stop(); // disposes the composable
      vi.advanceTimersByTime(10_000);
      // The timer was cleared on dispose, so it never nulled the notice.
      expect(actions.notice.value).toBe(pending);
    } finally {
      vi.useRealTimers();
    }
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

  it("focuses a stable control and still Escapes when an account has no folders (finding 7)", async () => {
    const empty: AccountSummary[] = [{ ...accounts[0]!, mailboxes: [] }];
    const wrapper = mount(MoveToFolderDialog, {
      props: { accounts: empty, ids: [7], mode: "move" },
      attachTo: document.body,
    });
    await nextTick();

    // With no folder buttons, focus falls back to the always-present Cancel so
    // the focus trap has an anchor.
    const cancel = wrapper
      .findAll("button.button")
      .find((b) => b.text() === "Cancel");
    expect(cancel).toBeDefined();
    expect(document.activeElement).toBe(cancel!.element);

    await wrapper.get(".overlay").trigger("keydown", { key: "Escape" });
    expect(wrapper.emitted("close")).toBeTruthy();
    wrapper.unmount();
  });
});

describe("RowContextMenu", () => {
  it("offers the relocation actions and emits the chosen one", () => {
    // The menu renders through a Teleport, so query the document body.
    const wrapper = mount(RowContextMenu, { props: { x: 5, y: 6 } });
    const items = [...document.body.querySelectorAll('[role="menuitem"]')];
    const labels = items.map((i) => i.textContent?.trim());
    expect(labels).toEqual([
      "Archive",
      "Move to…",
      "Copy to…",
      "Mark read",
      "Mark as junk",
      "Not junk",
      "Delete",
    ]);

    const del = items.find((i) => i.textContent?.trim() === "Delete");
    (del as HTMLElement).click();
    expect(wrapper.emitted("action")?.[0]).toEqual(["delete"]);
    expect(wrapper.emitted("close")).toBeTruthy();
    wrapper.unmount();
  });

  it("restores focus to the originating row when it closes (finding 7)", async () => {
    const trigger = document.createElement("button");
    document.body.appendChild(trigger);
    trigger.focus();
    expect(document.activeElement).toBe(trigger);

    const wrapper = mount(RowContextMenu, {
      props: { x: 5, y: 6 },
      attachTo: document.body,
    });
    await nextTick();
    // Focus moved onto a menu item on open.
    expect(
      (document.activeElement as HTMLElement | null)?.classList.contains("item"),
    ).toBe(true);

    // Escape asks to close (the menu renders through a Teleport, so dispatch on
    // the body node); the host unmounts, which restores focus to the opener.
    const menu = document.body.querySelector<HTMLElement>('[role="menu"]');
    menu?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await nextTick();
    expect(wrapper.emitted("close")).toBeTruthy();
    wrapper.unmount();
    expect(document.activeElement).toBe(trigger);
    trigger.remove();
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
