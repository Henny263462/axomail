<script setup lang="ts">
import { computed } from "vue";

import type { DialogController } from "../../composables/useDialogs";
import type { AccountSummary } from "../../types/mail";
import ActivityDialog from "../activity/ActivityDialog.vue";
import ComposerDialog from "../compose/ComposerDialog.vue";
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
}>();

// The composer is only mounted while open; the fallback keeps the prop types
// total without leaking a half-open state.
const composer = computed(
  () => props.dialogs.payload("composer") ?? { accountId: 0, draft: null },
);
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
  />
  <ActivityDialog
    v-if="dialogs.isOpen('activity')"
    @close="dialogs.close('activity')"
  />
</template>
