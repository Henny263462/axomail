/**
 * Pure quick-filter and sort logic for the message list.
 *
 * Every predicate here runs client-side over the already-loaded page(s) — it
 * never fetches. The facets mirror the fields the list DTO actually carries:
 * unread (`seen`), starred (`flagged`), tag membership, and a free-text match
 * over subject / sender / preview.
 *
 * A has-attachment facet is intentionally absent: attachment presence is not
 * indexed at ingest (the `attachments` table is unpopulated), so it cannot be
 * filtered honestly without a schema/ingest change that is out of this track's
 * no-migration scope. See the track notes.
 */
import type { MessageListItem } from "../../types/mail";

/** The active quick-filter facets. All default to "no constraint". */
export interface QuickFilter {
  /** Keep only unread (unseen) messages. */
  unread: boolean;
  /** Keep only starred (flagged) messages. */
  starred: boolean;
  /** Keep only messages carrying this tag id, or null for "any tag". */
  tagId: number | null;
  /** Free-text match over subject / sender / preview (case-insensitive). */
  text: string;
}

/** Field the list sorts on. */
export type SortField = "date" | "sender" | "subject";
/** Sort direction. */
export type SortDir = "asc" | "desc";

/** The active sort. */
export interface SortSpec {
  field: SortField;
  dir: SortDir;
}

/** The neutral filter: every facet off. */
export function emptyFilter(): QuickFilter {
  return { unread: false, starred: false, tagId: null, text: "" };
}

/** Whether any facet is constraining the list. */
export function isFilterActive(filter: QuickFilter): boolean {
  return (
    filter.unread ||
    filter.starred ||
    filter.tagId !== null ||
    filter.text.trim().length > 0
  );
}

/** The default sort: newest first. */
export function defaultSort(): SortSpec {
  return { field: "date", dir: "desc" };
}

/** The sender label a message sorts and filters on. */
function senderLabel(item: MessageListItem): string {
  return (item.fromName || item.fromAddress).toLowerCase();
}

/**
 * The base subject for sorting: a leading run of reply/forward prefixes
 * (`Re:`, `Fwd:`, `Fw:`) is stripped so replies sort next to their originals.
 */
export function baseSubject(subject: string | null): string {
  let text = (subject ?? "").trim();
  // Strip any leading chain of "Re:" / "Fwd:" / "Fw:" prefixes.
  const prefix = /^(re|fwd|fw)\s*:\s*/i;
  while (prefix.test(text)) text = text.replace(prefix, "").trim();
  return text.toLowerCase();
}

/** Whether `item` passes every active facet of `filter`. */
export function matchesFilter(
  item: MessageListItem,
  filter: QuickFilter,
): boolean {
  if (filter.unread && item.seen) return false;
  if (filter.starred && !item.flagged) return false;
  if (
    filter.tagId !== null &&
    !item.tags.some((tag) => tag.id === filter.tagId)
  ) {
    return false;
  }
  const needle = filter.text.trim().toLowerCase();
  if (needle.length > 0) {
    const haystack = [
      item.subject ?? "",
      item.fromName,
      item.fromAddress,
      item.preview ?? "",
    ]
      .join(" ")
      .toLowerCase();
    if (!haystack.includes(needle)) return false;
  }
  return true;
}

/** Filters `items` to those passing `filter` (input not mutated). */
export function filterMessages(
  items: readonly MessageListItem[],
  filter: QuickFilter,
): MessageListItem[] {
  return items.filter((item) => matchesFilter(item, filter));
}

/** Comparator for two messages under `sort`. */
export function compareMessages(
  a: MessageListItem,
  b: MessageListItem,
  sort: SortSpec,
): number {
  let ordered: number;
  switch (sort.field) {
    case "sender":
      ordered = senderLabel(a).localeCompare(senderLabel(b));
      break;
    case "subject":
      ordered = baseSubject(a.subject).localeCompare(baseSubject(b.subject));
      break;
    case "date":
    default: {
      const da = a.dateUnix ?? Number.NEGATIVE_INFINITY;
      const db = b.dateUnix ?? Number.NEGATIVE_INFINITY;
      ordered = da - db;
      break;
    }
  }
  // Break ties by id for a total, stable order across reloads.
  if (ordered === 0) ordered = a.id - b.id;
  return sort.dir === "asc" ? ordered : -ordered;
}

/** Sorts a copy of `items` under `sort`. */
export function sortMessages(
  items: readonly MessageListItem[],
  sort: SortSpec,
): MessageListItem[] {
  return [...items].sort((a, b) => compareMessages(a, b, sort));
}
