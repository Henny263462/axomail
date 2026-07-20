<script setup lang="ts">
import { computed } from "vue";

import type { DialogController } from "../../composables/useDialogs";
import type { FolderPicked } from "../../composables/useMailboxActions";
import type { AccountSummary, SearchResultDto } from "../../types/mail";
import ActivityDialog from "../activity/ActivityDialog.vue";
import CommandPalette from "../command/CommandPalette.vue";
import ShortcutHelp from "../command/ShortcutHelp.vue";
import ComposerDialog from "../compose/ComposerDialog.vue";
import TemplatePickerDialog from "../compose/TemplatePickerDialog.vue";
import AddressBookDialog from "../contacts/AddressBookDialog.vue";
import MoveToFolderDialog from "../message/MoveToFolderDialog.vue";
import AdvancedSearchDialog from "../search/AdvancedSearchDialog.vue";
import SettingsDialog from "../settings/SettingsDialog.vue";
import AccountSetupDialog from "../setup/AccountSetupDialog.vue";
import type { MessageTemplateDto } from "../../types/templates";
import type { AppDialogMap } from "./dialogs";

const props = defineProps<{
  /** Shared dialog open-state controller from {@link useDialogs}. */
  dialogs: DialogController<AppDialogMap>;
  /** Accounts, forwarded to the settings dialog. */
  accounts: AccountSummary[];
}>();

const emit = defineEmits<{
  accountCreated: [account: AccountSummary];
  composerSent: [payload: { draftId: number | null }];
  composerSaved: [];
  accountsChanged: [];
  tagsChanged: [];
  settingsSaved: [];
  folderPicked: [payload: FolderPicked];
  advancedSearchRun: [payload: { title: string; results: SearchResultDto[] }];
  savedSearchesChanged: [];
}>();

// The composer is only mounted while open; the fallback keeps the prop types
// total without leaking a half-open state.
const composer = computed(
  () =>
    props.dialogs.payload("composer") ?? {
      accountId: 0,
      draft: null,
      prefill: null,
      initial: null,
    },
);

// Same total-typing fallback for the move/copy picker payload.
const moveTarget = computed(
  () => props.dialogs.payload("moveToFolder") ?? { ids: [], mode: "move" as const },
);

// Total-typing fallback for the advanced-search payload (no saved search = new).
const advancedSearch = computed(
  () => props.dialogs.payload("advancedSearch") ?? { savedSearch: null },
);

/** Forwards a picked folder to the host and closes the picker. */
function onPicked(payload: FolderPicked): void {
  emit("folderPicked", payload);
  props.dialogs.close("moveToFolder");
}

/**
 * Opens the composer pre-filled from a chosen template. Templates are
 * account-agnostic, so the first account seeds the From field; the composer's
 * own From picker lets the user switch identity/account.
 */
function onTemplateSelected(template: MessageTemplateDto): void {
  props.dialogs.close("templates");
  props.dialogs.open("composer", {
    accountId: props.accounts[0]?.id ?? 0,
    draft: null,
    initial: {
      to: template.to,
      cc: template.cc,
      bcc: template.bcc,
      subject: template.subject,
      body: template.bodyText,
    },
  });
}
</script>

<template>
  <AccountSetupDialog
    v-if="dialogs.isOpen('setup')"
    @close="dialogs.close('setup')"
    @created="emit('accountCreated', $event)"
  />
  <ComposerDialog
    v-if="dialogs.isOpen('composer')"
    :account-id="composer.accountId"
    :draft="composer.draft"
    :prefill="composer.prefill ?? null"
    :initial="composer.initial ?? null"
    :accounts="accounts"
    @close="dialogs.close('composer')"
    @sent="emit('composerSent', $event)"
    @saved="emit('composerSaved')"
  />
  <TemplatePickerDialog
    v-if="dialogs.isOpen('templates')"
    @close="dialogs.close('templates')"
    @select="onTemplateSelected"
  />
  <SettingsDialog
    v-if="dialogs.isOpen('settings')"
    :accounts="accounts"
    @close="dialogs.close('settings')"
    @accounts-changed="emit('accountsChanged')"
    @tags-changed="emit('tagsChanged')"
    @settings-saved="emit('settingsSaved')"
  />
  <ActivityDialog
    v-if="dialogs.isOpen('activity')"
    @close="dialogs.close('activity')"
  />
  <AddressBookDialog
    v-if="dialogs.isOpen('addressBook')"
    @close="dialogs.close('addressBook')"
  />
  <MoveToFolderDialog
    v-if="dialogs.isOpen('moveToFolder')"
    :accounts="accounts"
    :ids="moveTarget.ids"
    :mode="moveTarget.mode"
    @close="dialogs.close('moveToFolder')"
    @picked="onPicked"
  />
  <CommandPalette
    v-if="dialogs.isOpen('commandPalette')"
    @close="dialogs.close('commandPalette')"
  />
  <ShortcutHelp
    v-if="dialogs.isOpen('shortcutHelp')"
    @close="dialogs.close('shortcutHelp')"
  />
  <AdvancedSearchDialog
    v-if="dialogs.isOpen('advancedSearch')"
    :saved-search="advancedSearch.savedSearch"
    :accounts="accounts"
    @close="dialogs.close('advancedSearch')"
    @run="emit('advancedSearchRun', $event)"
    @saved="emit('savedSearchesChanged')"
  />
</template>
