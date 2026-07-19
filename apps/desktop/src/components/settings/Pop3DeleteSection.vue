<script setup lang="ts">
/**
 * The "POP3 server deletion" settings section.
 *
 * Server-side deletion after download is POP3's one destructive operation, so
 * each account's toggle is wired to the dedicated `setPop3DeletePolicy`
 * command — the user-visible second delete guard — and applies immediately
 * rather than waiting for the dialog's Save. Failures are reported upward so
 * the dialog's single alert region announces them; on failure the checkbox
 * reverts and local state changes only after the backend accepts it.
 */
import { setPop3DeletePolicy } from "../../api/settings";
import { isTauri } from "../../lib/tauri";
import type { AccountSummary, Pop3DeletePolicyDto } from "../../types/mail";

/** Exact warning shown beside every POP3 server-deletion toggle. */
const DELETE_WARNING =
  "Deletes mail from the server after download. This cannot be undone.";

const props = defineProps<{
  /** The POP3 accounts' current delete policies (mutated in place on success). */
  policies: Pop3DeletePolicyDto[];
  /** Accounts, used to label each toggle. */
  accounts: AccountSummary[];
}>();

const emit = defineEmits<{
  /** A user-facing error string for the dialog's alert region ("" clears it). */
  error: [message: string];
}>();

function accountName(accountId: number): string {
  const account = props.accounts.find((item) => item.id === accountId);
  return account?.displayName ?? `Account ${accountId}`;
}

function messageFrom(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Could not change the delete policy.";
}

async function onToggle(
  policy: Pop3DeletePolicyDto,
  event: Event,
): Promise<void> {
  const next = (event.target as HTMLInputElement).checked;
  if (!isTauri()) {
    (event.target as HTMLInputElement).checked = policy.enabled;
    emit(
      "error",
      "Changing the delete policy needs the desktop app; it is unavailable in the browser preview.",
    );
    return;
  }
  try {
    await setPop3DeletePolicy(policy.accountId, next);
    policy.enabled = next;
    emit("error", "");
  } catch (error) {
    (event.target as HTMLInputElement).checked = policy.enabled;
    emit("error", messageFrom(error));
  }
}
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      POP3 server deletion
    </legend>
    <p class="warning">
      {{ DELETE_WARNING }}
    </p>
    <label
      v-for="policy in policies"
      :key="policy.accountId"
      class="toggle"
    >
      <input
        type="checkbox"
        :checked="policy.enabled"
        @change="onToggle(policy, $event)"
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
