/**
 * The top-level views the shell can show. Lives outside the SFC so both
 * {@link SidebarPane} and its host can share the type without a
 * `<script setup>` export (which the Vue compiler disallows).
 *
 * `unified` is the fixed "All inboxes" view that merges every account's
 * inbox-role mailbox.
 */
export type SidebarView = "mailbox" | "unified" | "drafts" | "outbox";
