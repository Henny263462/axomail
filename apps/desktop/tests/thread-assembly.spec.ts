import { describe, expect, it } from "vitest";

import { assembleThreads } from "../src/components/list/threadAssembly";
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

describe("assembleThreads grouping", () => {
  it("groups a reply chain by References / Message-ID into one thread", () => {
    const items = [
      msg({ id: 1, messageId: "a", dateUnix: 100, seen: true }),
      msg({
        id: 2,
        messageId: "b",
        inReplyTo: ["a"],
        references: ["a"],
        dateUnix: 200,
        seen: false,
      }),
      msg({ id: 3, messageId: "c", references: ["a", "b"], dateUnix: 300, seen: false }),
      // An unrelated message forms its own singleton thread.
      msg({ id: 4, messageId: "x", dateUnix: 50, seen: true }),
    ];

    const threads = assembleThreads(items);
    expect(threads).toHaveLength(2);

    // Threads are ordered newest-first by their latest message.
    const [conversation, singleton] = threads;
    expect(conversation!.latest.id).toBe(3);
    expect(conversation!.count).toBe(3);
    expect(conversation!.unread).toBe(2);
    expect(conversation!.id).toBe(1); // lowest member id
    // Members are newest-first.
    expect(conversation!.messages.map((m) => m.id)).toEqual([3, 2, 1]);

    expect(singleton!.count).toBe(1);
    expect(singleton!.latest.id).toBe(4);
    expect(singleton!.unread).toBe(0);
  });

  it("merges duplicate Message-IDs (same message in two mailboxes)", () => {
    const items = [
      msg({ id: 5, messageId: "dup" }),
      msg({ id: 6, messageId: "dup" }),
    ];
    const threads = assembleThreads(items);
    expect(threads).toHaveLength(1);
    expect(threads[0]!.count).toBe(2);
  });

  it("leaves header-less messages as singleton threads (no subject merge)", () => {
    const items = [
      msg({ id: 1, subject: "Re: hi", messageId: null }),
      msg({ id: 2, subject: "Re: hi", messageId: null }),
    ];
    const threads = assembleThreads(items);
    // Same subject but no shared reference id → deliberately NOT merged.
    expect(threads).toHaveLength(2);
  });

  it("threads a reply whose parent is absent from the page as its own thread", () => {
    const items = [
      msg({ id: 9, messageId: "child", references: ["missing-parent"] }),
    ];
    const threads = assembleThreads(items);
    expect(threads).toHaveLength(1);
    expect(threads[0]!.id).toBe(9);
  });
});
