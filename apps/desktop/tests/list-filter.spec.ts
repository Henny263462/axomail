import { describe, expect, it } from "vitest";

import {
  baseSubject,
  filterMessages,
  emptyFilter,
  isFilterActive,
  sortMessages,
  type QuickFilter,
  type SortSpec,
} from "../src/components/list/listFilter";
import type { MessageListItem, TagChip } from "../src/types/mail";

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

const work: TagChip = { id: 7, displayName: "Work", color: null };

function withFilter(patch: Partial<QuickFilter>): QuickFilter {
  return { ...emptyFilter(), ...patch };
}

describe("quick-filter facets", () => {
  const rows = [
    msg({ id: 1, seen: false, flagged: false, subject: "Alpha report" }),
    msg({ id: 2, seen: true, flagged: true, subject: "Beta note", tags: [work] }),
    msg({ id: 3, seen: true, flagged: false, fromName: "Zoe", preview: "gamma" }),
  ];

  it("keeps only unread messages", () => {
    expect(filterMessages(rows, withFilter({ unread: true })).map((m) => m.id)).toEqual([1]);
  });

  it("keeps only starred messages", () => {
    expect(filterMessages(rows, withFilter({ starred: true })).map((m) => m.id)).toEqual([2]);
  });

  it("keeps only messages carrying the chosen tag", () => {
    expect(filterMessages(rows, withFilter({ tagId: 7 })).map((m) => m.id)).toEqual([2]);
  });

  it("matches free text against subject, sender, and preview", () => {
    expect(filterMessages(rows, withFilter({ text: "alpha" })).map((m) => m.id)).toEqual([1]);
    expect(filterMessages(rows, withFilter({ text: "zoe" })).map((m) => m.id)).toEqual([3]);
    expect(filterMessages(rows, withFilter({ text: "gamma" })).map((m) => m.id)).toEqual([3]);
  });

  it("combines facets (unread AND text)", () => {
    const both = withFilter({ unread: true, text: "report" });
    expect(filterMessages(rows, both).map((m) => m.id)).toEqual([1]);
  });

  it("reports whether any facet is active", () => {
    expect(isFilterActive(emptyFilter())).toBe(false);
    expect(isFilterActive(withFilter({ unread: true }))).toBe(true);
    expect(isFilterActive(withFilter({ text: "  " }))).toBe(false);
  });
});

describe("sort", () => {
  const rows = [
    msg({ id: 1, dateUnix: 300, fromName: "Bob", subject: "Re: Charlie" }),
    msg({ id: 2, dateUnix: 100, fromName: "ada", subject: "Alpha" }),
    msg({ id: 3, dateUnix: 200, fromName: "Cara", subject: "beta" }),
  ];

  function ids(sort: SortSpec): number[] {
    return sortMessages(rows, sort).map((m) => m.id);
  }

  it("sorts by date, both directions", () => {
    expect(ids({ field: "date", dir: "desc" })).toEqual([1, 3, 2]);
    expect(ids({ field: "date", dir: "asc" })).toEqual([2, 3, 1]);
  });

  it("sorts by sender case-insensitively", () => {
    expect(ids({ field: "sender", dir: "asc" })).toEqual([2, 1, 3]);
  });

  it("sorts by subject, ignoring Re:/Fwd: prefixes", () => {
    // "Alpha" < "beta" < "Charlie" (from "Re: Charlie").
    expect(ids({ field: "subject", dir: "asc" })).toEqual([2, 3, 1]);
  });

  it("strips a chain of reply/forward prefixes for the base subject", () => {
    expect(baseSubject("Re: Fwd: Re: Hello")).toBe("hello");
    expect(baseSubject(null)).toBe("");
  });
});
