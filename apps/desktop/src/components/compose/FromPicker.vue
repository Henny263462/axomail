<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { AccountSummary, IdentityDto } from "../../types/mail";

const props = defineProps<{
  /** Identities to choose from, already ordered account-then-default-first. */
  identities: IdentityDto[];
  /** Currently selected identity id, or null when none is chosen. */
  modelValue: number | null;
  /** Accounts, used only to label each identity group. */
  accounts?: AccountSummary[];
}>();

const emit = defineEmits<{
  "update:modelValue": [identityId: number];
}>();

const { t } = useI18n({ useScope: "global" });

interface IdentityGroup {
  accountId: number;
  label: string;
  identities: IdentityDto[];
}

/** Groups identities by account, preserving their incoming order. */
const groups = computed<IdentityGroup[]>(() => {
  const byAccount = new Map<number, IdentityDto[]>();
  for (const identity of props.identities) {
    const list = byAccount.get(identity.accountId) ?? [];
    list.push(identity);
    byAccount.set(identity.accountId, list);
  }
  return Array.from(byAccount, ([accountId, list]) => ({
    accountId,
    label: accountLabel(accountId),
    identities: list,
  }));
});

function accountLabel(accountId: number): string {
  const account = props.accounts?.find((item) => item.id === accountId);
  return account?.displayName ?? t("compose.from.accountFallback", { id: accountId });
}

/** Renders an identity as `Display Name <address>` or just the address. */
function optionLabel(identity: IdentityDto): string {
  const name = identity.displayName.trim();
  return name.length > 0 ? `${name} <${identity.email}>` : identity.email;
}

function onChange(event: Event): void {
  const value = Number((event.target as HTMLSelectElement).value);
  emit("update:modelValue", value);
}
</script>

<template>
  <div class="field">
    <label for="compose-from">{{ t("compose.from.label") }}</label>
    <select
      id="compose-from"
      :value="modelValue ?? ''"
      @change="onChange"
    >
      <optgroup
        v-for="group in groups"
        :key="group.accountId"
        :label="group.label"
      >
        <option
          v-for="identity in group.identities"
          :key="identity.id"
          :value="identity.id"
        >
          {{ optionLabel(identity) }}
        </option>
      </optgroup>
    </select>
  </div>
</template>

<style scoped>
.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.field label {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

.field select {
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
}

.field select:focus-visible {
  border-color: var(--color-accent);
}
</style>
