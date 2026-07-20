import type { Component } from "vue";

import { i18n } from "../../i18n";
import AccountsSection from "./AccountsSection.vue";
import AppearanceSection from "./AppearanceSection.vue";
import AutoReadSection from "./AutoReadSection.vue";
import ContactsSection from "./ContactsSection.vue";
import EmptyTrashSection from "./EmptyTrashSection.vue";
import FiltersSection from "./FiltersSection.vue";
import IdentitiesSection from "./IdentitiesSection.vue";
import JunkSection from "./JunkSection.vue";
import LanguageSection from "./LanguageSection.vue";
import NotificationsSection from "./NotificationsSection.vue";
import PgpKeysSection from "./PgpKeysSection.vue";
import Pop3DeleteSection from "./Pop3DeleteSection.vue";
import PushSection from "./PushSection.vue";
import ReceiptsSection from "./ReceiptsSection.vue";
import ShortcutsSection from "./ShortcutsSection.vue";
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
  /**
   * Human-readable section name (for docs and the section's own chrome). A
   * live getter resolving `settings.sections.<key>`, so a runtime locale switch
   * re-localizes it — never a frozen English string.
   */
  readonly title: string;
  /** The section body component (props: `{ context }`). */
  component: Component;
}

/**
 * Builds a section whose `title` resolves from the i18n catalogue on every
 * access (via `settings.sections.<key>`), keeping it reactive to a locale
 * switch while still satisfying `title: string`.
 */
function section(id: string, key: string, component: Component): SettingsSection {
  return {
    id,
    get title() {
      return i18n.global.t(`settings.sections.${key}`);
    },
    component,
  };
}

/**
 * The ordered settings sections. Order here is render order in the dialog.
 * Later tracks (dark mode, identities, account management, remote content)
 * register by creating a `context`-driven SFC and appending one entry here.
 */
export const settingsSections: SettingsSection[] = [
  section("notifications", "notifications", NotificationsSection),
  section("sync-interval", "syncInterval", SyncIntervalSection),
  section("push", "push", PushSection),
  section("auto-read", "autoRead", AutoReadSection),
  section("receipts", "receipts", ReceiptsSection),
  section("appearance", "appearance", AppearanceSection),
  section("language", "language", LanguageSection),
  section("pop3-delete", "pop3Delete", Pop3DeleteSection),
  section("empty-trash-on-exit", "emptyTrash", EmptyTrashSection),
  section("identities", "identities", IdentitiesSection),
  section("tags", "tags", TagsSection),
  section("contacts", "contacts", ContactsSection),
  section("filters", "filters", FiltersSection),
  section("junk", "junk", JunkSection),
  section("pgp", "pgp", PgpKeysSection),
  section("shortcuts", "shortcuts", ShortcutsSection),
  section("accounts", "accounts", AccountsSection),
];
