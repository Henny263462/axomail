<script setup lang="ts">
import { computed } from "vue";

import type { DialogController } from "../../composables/useDialogs";
import type { FolderPicked } from "../../composables/useMailboxActions";
import type { AccountSummary } from "../../types/mail";
import ActivityDialog from "../activity/ActivityDialog.vue";
import ComposerDialog from "../compose/ComposerDialog.vue";
import AddressBookDialog from "../contacts/AddressBookDialog.vue";
import MoveToFolderDialog from "../message/MoveToFolderDialog.vue";
import SettingsDialog from "../settings/SettingsDialog.vue";
import AccountSetupDialog from "../setup/AccountSetupDialog.vue";
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
}>();

// The composer is only mounted while open; the fallback keeps the prop types
// total without leaking a half-open state.
const composer = computed(
  () =>
    props.dialogs.payload("composer") ?? {
      accountId: 0,
      draft: null,
      prefill: null,
    },
);

// Same total-typing fallback for the move/copy picker payload.
const moveTarget = computed(
  () => props.dialogs.payload("moveToFolder") ?? { ids: [], mode: "move" as const },
);

/** Forwards a picked folder to the host and closes the picker. */
function onPicked(payload: FolderPicked): void {
  emit("folderPicked", payload);
  props.dialogs.close("moveToFolder");
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
    :accounts="accounts"
    @close="dialogs.close('composer')"
    @sent="emit('composerSent', $event)"
    @saved="emit('composerSaved')"
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
</template>
