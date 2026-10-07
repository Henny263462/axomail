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
import ViewSection from "./ViewSection.vue";
import type { IconName } from "../common/icons";

export type SettingsGroupId =
  | "general"
  | "appearance"
  | "accounts"
  | "mail"
  | "contacts"
  | "security"
  | "shortcuts";

export interface SettingsGroup {
  id: SettingsGroupId;
  icon: IconName;
  readonly title: string;
}

function group(id: SettingsGroupId, icon: IconName): SettingsGroup {
  return {
    id,
    icon,
    get title() {
      return i18n.global.t(`settings.groups.${id}`);
    },
  };
}

export const settingsGroups: SettingsGroup[] = [
  group("general", "settings"),
  group("appearance", "palette"),
  group("accounts", "user"),
  group("mail", "filter"),
  group("contacts", "book"),
  group("security", "shield"),
  group("shortcuts", "keyboard"),
];

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
  group: SettingsGroupId;
}

/**
 * Builds a section whose `title` resolves from the i18n catalogue on every
 * access (via `settings.sections.<key>`), keeping it reactive to a locale
 * switch while still satisfying `title: string`.
 */
function section(
  id: string,
  key: string,
  component: Component,
  groupId: SettingsGroupId,
): SettingsSection {
  return {
    id,
    group: groupId,
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
  section("notifications", "notifications", NotificationsSection, "general"),
  section("sync-interval", "syncInterval", SyncIntervalSection, "general"),
  section("push", "push", PushSection, "general"),
  section("auto-read", "autoRead", AutoReadSection, "general"),
  section("language", "language", LanguageSection, "general"),
  section("appearance", "appearance", AppearanceSection, "appearance"),
  section("view", "view", ViewSection, "appearance"),
  section("accounts", "accounts", AccountsSection, "accounts"),
  section("identities", "identities", IdentitiesSection, "accounts"),
  section("pop3-delete", "pop3Delete", Pop3DeleteSection, "accounts"),
  section("empty-trash-on-exit", "emptyTrash", EmptyTrashSection, "accounts"),
  section("receipts", "receipts", ReceiptsSection, "mail"),
  section("tags", "tags", TagsSection, "mail"),
  section("filters", "filters", FiltersSection, "mail"),
  section("junk", "junk", JunkSection, "mail"),
  section("contacts", "contacts", ContactsSection, "contacts"),
  section("pgp", "pgp", PgpKeysSection, "security"),
  section("shortcuts", "shortcuts", ShortcutsSection, "shortcuts"),
];
