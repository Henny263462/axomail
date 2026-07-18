/**
 * The three top-level views the shell can show. Lives outside the SFC so
 * both {@link SidebarPane} and its host can share the type without a
 * `<script setup>` export (which the Vue compiler disallows).
 */
export type SidebarView = "mailbox" | "drafts" | "outbox";
