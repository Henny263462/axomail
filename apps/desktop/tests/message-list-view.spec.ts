import { nextTick, ref } from "vue";
import { describe, expect, it } from "vitest";

import { useMessageListView } from "../src/composables/useMessageListView";
import { messageRowId, threadRowId } from "../src/types/selection";
import type { MessageListItem } from "../src/types/mail";

function msg(over: Partial<MessageListItem> & { id: number }): MessageListItem {
  return {
    mailboxId: 10,
    subject: "Subject",
    fromName: "Ada",
    fromAddress: "ada@example.org",
    dateUnix: over.id,
    seen: true,
    flagged: false,
    preview: null,
    tags: [],
    ...over,
  };
}

/** A two-message thread (ids 1,2) plus a lone message (id 3). */
function threadedSet(): MessageListItem[] {
  return [
    msg({ id: 1, messageId: "a", dateUnix: 100, seen: true }),
    msg({ id: 2, messageId: "b", references: ["a"], dateUnix: 200, seen: false }),
    msg({ id: 3, messageId: "c", dateUnix: 50, seen: true }),
  ];
}

describe("useMessageListView flat mode", () => {
  it("renders every message as a row in sorted order", () => {
    const messages = ref(threadedSet());
    const view = useMessageListView({ messages, threaded: ref(false) });
    expect(view.visibleRows.value.every((r) => r.kind === "message")).toBe(true);
    // Default sort is newest-first: 2 (200), 1 (100), 3 (50).
    expect(view.rowIds.value).toEqual([
      messageRowId(2),
      messageRowId(1),
      messageRowId(3),
    ]);
  });

  it("filters the loaded set client-side", () => {
    const messages = ref(threadedSet());
    const view = useMessageListView({ messages, threaded: ref(false) });
    view.filter.value = { ...view.filter.value, unread: true };
    expect(view.rowIds.value).toEqual([messageRowId(2)]);
  });
});

describe("useMessageListView threaded mode", () => {
  it("collapses a conversation to a thread row with a rollup", () => {
    const messages = ref(threadedSet());
    const view = useMessageListView({ messages, threaded: ref(true) });
    // Thread (2 msgs) collapsed, then the lone message.
    expect(view.rowIds.value).toEqual([threadRowId(1), messageRowId(3)]);
    const threadRow = view.visibleRows.value[0];
    if (threadRow?.kind !== "thread") throw new Error("expected a thread row");
    expect(threadRow.count).toBe(2);
    expect(threadRow.unread).toBe(1);
    expect(threadRow.latest.id).toBe(2);
  });

  it("expands a thread to reveal its older replies", () => {
    const messages = ref(threadedSet());
    const view = useMessageListView({ messages, threaded: ref(true) });
    view.toggleThread(1);
    // Thread row, its older child (id 1), then the lone message.
    expect(view.rowIds.value).toEqual([
      threadRowId(1),
      messageRowId(1),
      messageRowId(3),
    ]);
  });

  it("resolves a selected thread to all its member message ids", () => {
    const messages = ref(threadedSet());
    const view = useMessageListView({ messages, threaded: ref(true) });
    view.selection.selectOnly(threadRowId(1));
    // Newest-first members of the thread.
    expect(view.selectedMessageIds.value).toEqual([2, 1]);
  });

  it("supports range and toggle selection across threaded rows", () => {
    const messages = ref(threadedSet());
    const view = useMessageListView({ messages, threaded: ref(true) });
    view.toggleThread(1); // rows: t:1, m:1, m:3

    // Range from the thread row to the lone message selects everything between.
    view.selection.select(threadRowId(1));
    view.selection.select(messageRowId(3), { range: true });
    expect(view.selection.selectedIds.value).toEqual([
      threadRowId(1),
      messageRowId(1),
      messageRowId(3),
    ]);
    // Deduplicated member ids, in visible order.
    expect(view.selectedMessageIds.value).toEqual([2, 1, 3]);

    // Ctrl-toggle removes the lone message again.
    view.selection.select(messageRowId(3), { toggle: true });
    expect(view.selectedMessageIds.value).toEqual([2, 1]);
  });

  it("drops a selected row that vanishes when the list reloads", async () => {
    const messages = ref(threadedSet());
    const view = useMessageListView({ messages, threaded: ref(false) });
    view.selection.select(messageRowId(3));
    expect(view.selectedMessageIds.value).toEqual([3]);

    // Message 3 leaves the loaded set; the selection reconciles.
    messages.value = messages.value.filter((m) => m.id !== 3);
    await nextTick();
    expect(view.selectedMessageIds.value).toEqual([]);
  });
});
