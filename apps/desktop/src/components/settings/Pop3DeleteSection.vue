<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";

import { setPop3DeletePolicy } from "../../api/settings";
import { isTauri } from "../../lib/tauri";
import type { Pop3DeletePolicyDto } from "../../types/mail";
import { messageFrom, type SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

const { t } = useI18n({ useScope: "global" });

// Local, mutable copy so a rejected toggle can revert its checkbox without
// touching the dialog's loaded snapshot (which it echoes back on save).
const policies = ref<Pop3DeletePolicyDto[]>(
  props.context.initialPolicies.map((policy) => ({ ...policy })),
);

function accountName(accountId: number): string {
  const account = props.context.accounts.find((item) => item.id === accountId);
  return account?.displayName ?? t("settings.pop3.accountFallback", { id: accountId });
}

/**
 * Flips one account's POP3 delete policy through the dedicated command — the
 * user-visible second guard. On failure the checkbox reverts and the error is
 * announced; local state changes only after the backend accepts it.
 */
async function onTogglePolicy(
  policy: Pop3DeletePolicyDto,
  event: Event,
): Promise<void> {
  const next = (event.target as HTMLInputElement).checked;
  props.context.reportStatus("");
  if (!isTauri()) {
    (event.target as HTMLInputElement).checked = policy.enabled;
    props.context.reportError(t("settings.pop3.unavailable"));
    return;
  }
  try {
    await setPop3DeletePolicy(policy.accountId, next);
    policy.enabled = next;
    props.context.reportError("");
  } catch (error) {
    (event.target as HTMLInputElement).checked = policy.enabled;
    props.context.reportError(messageFrom(error));
  }
}
</script>

<template>
  <fieldset
    v-if="policies.length > 0"
    class="group"
  >
    <legend class="legend">
      {{ t("settings.pop3.title") }}
    </legend>
    <p class="warning">
      {{ t("settings.pop3.warning") }}
    </p>
    <label
      v-for="policy in policies"
      :key="policy.accountId"
      class="toggle"
    >
      <input
        type="checkbox"
        :checked="policy.enabled"
        @change="onTogglePolicy(policy, $event)"
      >
      <span>{{ accountName(policy.accountId) }}</span>
    </label>
  </fieldset>
</template>

<style scoped>
.group {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.legend {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 0 var(--space-1);
}

.warning {
  font-size: var(--text-xs);
  color: var(--color-danger);
}

.toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
}
</style>
