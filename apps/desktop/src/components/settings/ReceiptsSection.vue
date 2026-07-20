<script setup lang="ts">
/**
 * The "Read receipts" settings section: the honour policy (never / ask /
 * always) plus two conditions (only when addressed to me, never for mailing
 * lists).
 *
 * An autonomous registry section (like Push / Language): its only prop is the
 * shared {@link SettingsSectionContext}. It loads the settings on mount and
 * persists the whole object immediately on any change through its own
 * `set_receipt_settings` command, so it does not widen the dialog's batch-saved
 * surface. A failed persist reverts and announces the error.
 */
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import { getReceiptSettings, setReceiptSettings } from "../../api/receipts";
import type { ReceiptPolicy, ReceiptSettings } from "../../types/receipts";
import { isTauri } from "../../lib/tauri";
import { messageFrom, type SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

const { t } = useI18n({ useScope: "global" });

const settings = ref<ReceiptSettings>({
  policy: "ask",
  onlyWhenAddressed: true,
  neverForMailingLists: true,
});
const loaded = ref(false);

const policyOptions: { value: ReceiptPolicy; labelKey: string }[] = [
  { value: "never", labelKey: "receipts.settings.policyNever" },
  { value: "ask", labelKey: "receipts.settings.policyAsk" },
  { value: "always", labelKey: "receipts.settings.policyAlways" },
];

onMounted(async () => {
  const value = await getReceiptSettings();
  if (value !== null) settings.value = value;
  loaded.value = true;
});

/** Persists a candidate settings object, reverting to `previous` on failure. */
async function persist(
  next: ReceiptSettings,
  previous: ReceiptSettings,
): Promise<void> {
  props.context.reportStatus("");
  if (!isTauri()) {
    props.context.reportError(t("receipts.settings.statusUnavailable"));
    return;
  }
  settings.value = next;
  try {
    await setReceiptSettings(next);
    props.context.reportError("");
    props.context.reportStatus(t("receipts.settings.statusSaved"));
  } catch (error) {
    settings.value = previous;
    props.context.reportError(messageFrom(error));
  }
}

function selectPolicy(policy: ReceiptPolicy): void {
  if (policy === settings.value.policy) return;
  void persist({ ...settings.value, policy }, settings.value);
}

function toggleOnlyWhenAddressed(event: Event): void {
  const onlyWhenAddressed = (event.target as HTMLInputElement).checked;
  void persist({ ...settings.value, onlyWhenAddressed }, settings.value);
}

function toggleNeverForLists(event: Event): void {
  const neverForMailingLists = (event.target as HTMLInputElement).checked;
  void persist({ ...settings.value, neverForMailingLists }, settings.value);
}
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      {{ t("receipts.settings.title") }}
    </legend>
    <p class="hint">
      {{ t("receipts.settings.hint") }}
    </p>
    <label
      v-for="option in policyOptions"
      :key="option.value"
      class="option"
    >
      <input
        type="radio"
        name="settings-receipt-policy"
        :value="option.value"
        :checked="settings.policy === option.value"
        :disabled="!loaded"
        @change="selectPolicy(option.value)"
      >
      <span>{{ t(option.labelKey) }}</span>
    </label>

    <label class="toggle">
      <input
        type="checkbox"
        :checked="settings.onlyWhenAddressed"
        :disabled="!loaded"
        @change="toggleOnlyWhenAddressed($event)"
      >
      <span>{{ t("receipts.settings.onlyWhenAddressed") }}</span>
    </label>
    <label class="toggle">
      <input
        type="checkbox"
        :checked="settings.neverForMailingLists"
        :disabled="!loaded"
        @change="toggleNeverForLists($event)"
      >
      <span>{{ t("receipts.settings.neverForLists") }}</span>
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

.hint {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.option,
.toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
}
</style>
