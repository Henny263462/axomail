/**
 * Development-only UI sample data (`dev-fixtures`).
 *
 * Loaded exclusively through a dynamic import guarded by
 * `import.meta.env.DEV` plus the `VITE_AXOMAIL_DEV_FIXTURES` flag (see
 * App.vue), so release builds neither bundle nor execute this module.
 * CI greps the production bundle for fixture markers and fails on a
 * leak (see .github/workflows/ci.yml, frontend job).
 * This data never touches the real data path; it only fills view models.
 */
import type { AccountSummary, MessageListItem } from "../types/mail";

export const fixtureAccounts: AccountSummary[] = [
  {
    id: 1,
    displayName: "Fixture account",
    email: "fixture@example.org",
    authMechanism: "password",
    mailboxes: [
      { id: 1, name: "INBOX", role: "inbox", unreadCount: 2, totalCount: 12, parentId: null, delimiter: "/", subscribed: true, orphaned: false },
      { id: 6, name: "INBOX/Projects", role: "custom", unreadCount: 1, totalCount: 4, parentId: 1, delimiter: "/", subscribed: true, orphaned: false },
      { id: 7, name: "INBOX/Projects/2026", role: "custom", unreadCount: 0, totalCount: 2, parentId: 6, delimiter: "/", subscribed: true, orphaned: false },
      { id: 2, name: "Drafts", role: "drafts", unreadCount: 0, totalCount: 1, parentId: null, delimiter: "/", subscribed: true, orphaned: false },
      { id: 3, name: "Sent", role: "sent", unreadCount: 0, totalCount: 30, parentId: null, delimiter: "/", subscribed: true, orphaned: false },
      { id: 4, name: "Archive", role: "archive", unreadCount: 0, totalCount: 88, parentId: null, delimiter: "/", subscribed: true, orphaned: false },
      { id: 5, name: "Trash", role: "trash", unreadCount: 0, totalCount: 3, parentId: null, delimiter: "/", subscribed: true, orphaned: false },
    ],
  },
];

export const fixtureMessages: MessageListItem[] = [
  {
    id: 101,
    mailboxId: 1,
    subject: "Design tokens review",
    fromName: "Ada Lovelace",
    fromAddress: "ada@example.org",
    dateUnix: 1_752_790_000,
    seen: false,
    flagged: true,
    preview: "The white-mode palette looks calm; two border values feel close…",
  },
  {
    id: 102,
    mailboxId: 1,
    subject: "Sync engine notes",
    fromName: "Grace Hopper",
    fromAddress: "grace@example.org",
    dateUnix: 1_752_700_000,
    seen: false,
    flagged: false,
    preview: "Backoff windows should be jittered; see the attached sketch…",
  },
  {
    id: 103,
    mailboxId: 1,
    subject: "Re: UIDVALIDITY handling",
    fromName: "Katherine Johnson",
    fromAddress: "katherine@example.org",
    dateUnix: 1_752_610_000,
    seen: true,
    flagged: false,
    preview: "A changed validity must invalidate every cached UID mapping…",
  },
];
