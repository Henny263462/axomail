import type { Component } from "vue";

import AccountsSection from "./AccountsSection.vue";
import AppearanceSection from "./AppearanceSection.vue";
import AutoReadSection from "./AutoReadSection.vue";
import ContactsSection from "./ContactsSection.vue";
import EmptyTrashSection from "./EmptyTrashSection.vue";
import FiltersSection from "./FiltersSection.vue";
import IdentitiesSection from "./IdentitiesSection.vue";
import NotificationsSection from "./NotificationsSection.vue";
import Pop3DeleteSection from "./Pop3DeleteSection.vue";
import SyncIntervalSection from "./SyncIntervalSection.vue";
import TagsSection from "./TagsSection.vue";

/**
 * One registered settings section. `component` renders inside the dialog's
 * form and receives the shared {@link SettingsSectionContext} as its single
 * `context` prop. Adding a section is: one new SFC plus one entry below.
 */
export interface SettingsSection {
  /** Stable identifier; also the section's list key. */
  id: string;
  /** Human-readable section name (for docs and the section's own chrome). */
  title: string;
  /** The section body component (props: `{ context }`). */
  component: Component;
}

/**
 * The ordered settings sections. Order here is render order in the dialog.
 * Later tracks (dark mode, identities, account management, remote content)
 * register by creating a `context`-driven SFC and appending one entry here.
 */
export const settingsSections: SettingsSection[] = [
  {
    id: "notifications",
    title: "Notifications",
    component: NotificationsSection,
  },
  { id: "sync-interval", title: "Sync", component: SyncIntervalSection },
  { id: "auto-read", title: "Reading", component: AutoReadSection },
  { id: "appearance", title: "Appearance", component: AppearanceSection },
  {
    id: "pop3-delete",
    title: "POP3 server deletion",
    component: Pop3DeleteSection,
  },
  {
    id: "empty-trash-on-exit",
    title: "Empty Trash on exit",
    component: EmptyTrashSection,
  },
  { id: "identities", title: "Identities", component: IdentitiesSection },
  { id: "tags", title: "Tags", component: TagsSection },
  { id: "contacts", title: "Contacts", component: ContactsSection },
  { id: "filters", title: "Filters", component: FiltersSection },
  { id: "accounts", title: "Accounts", component: AccountsSection },
];
