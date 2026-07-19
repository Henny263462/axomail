/**
 * Builds a nested folder tree from the flat mailbox list a
 * {@link MailboxSummary} account carries.
 *
 * Nesting follows `parentId`; a folder whose parent is missing (or is itself)
 * becomes a root. Each node caches its display `leaf` (the last path segment,
 * split on the folder's `delimiter`) and its `depth` for indentation. Siblings
 * are ordered by well-known role first, then case-insensitively by leaf, so the
 * tree reads like a familiar mail client.
 */
import type { MailboxSummary } from "../../types/mail";

export interface FolderTreeNode {
  mailbox: MailboxSummary;
  /** Zero-based depth for indentation (0 = top level). */
  depth: number;
  /** Display label: the last path segment. */
  leaf: string;
  children: FolderTreeNode[];
}

/** Display priority of well-known roles; lower sorts first. */
const ROLE_ORDER: Record<MailboxSummary["role"], number> = {
  inbox: 0,
  drafts: 1,
  sent: 2,
  archive: 3,
  junk: 4,
  trash: 5,
  custom: 6,
};

/** The last path segment of a folder, split on its delimiter. */
export function leafName(mailbox: MailboxSummary): string {
  const delimiter = mailbox.delimiter ?? null;
  if (delimiter !== null && delimiter.length > 0) {
    const parts = mailbox.name.split(delimiter);
    const last = parts[parts.length - 1];
    if (last !== undefined && last.length > 0) return last;
  }
  return mailbox.name;
}

/** Builds the ordered forest of folder nodes for one account. */
export function buildFolderTree(mailboxes: MailboxSummary[]): FolderTreeNode[] {
  const nodes = new Map<number, FolderTreeNode>();
  for (const mailbox of mailboxes) {
    nodes.set(mailbox.id, {
      mailbox,
      depth: 0,
      leaf: leafName(mailbox),
      children: [],
    });
  }

  const roots: FolderTreeNode[] = [];
  for (const mailbox of mailboxes) {
    const node = nodes.get(mailbox.id);
    if (node === undefined) continue;
    const parentId = mailbox.parentId ?? null;
    const parent =
      parentId !== null && parentId !== mailbox.id
        ? nodes.get(parentId)
        : undefined;
    if (parent !== undefined) {
      parent.children.push(node);
    } else {
      roots.push(node);
    }
  }

  sortAndDepth(roots, 0);
  return roots;
}

/** Recursively sorts each sibling list and stamps each node's depth. */
function sortAndDepth(list: FolderTreeNode[], depth: number): void {
  list.sort((a, b) => {
    const byRole = ROLE_ORDER[a.mailbox.role] - ROLE_ORDER[b.mailbox.role];
    if (byRole !== 0) return byRole;
    return a.leaf.localeCompare(b.leaf, undefined, { sensitivity: "base" });
  });
  for (const node of list) {
    node.depth = depth;
    sortAndDepth(node.children, depth + 1);
  }
}
