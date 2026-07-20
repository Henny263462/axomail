<script setup lang="ts">
/**
 * The recipient and subject fields of the composer (To / Cc / Bcc / Subject).
 *
 * Each field is a two-way `v-model` binding so the shell keeps owning the
 * values; this component only renders the inputs and their labels. To/Cc/Bcc use
 * {@link RecipientInput}, which adds address-book autocomplete over each field's
 * last token. It exposes {@link focus} so the shell can place the initial focus
 * on the To field when the dialog opens (the focus trap lives in the shell).
 */
import { ref } from "vue";

import RecipientInput from "./RecipientInput.vue";

const to = defineModel<string>("to", { default: "" });
const cc = defineModel<string>("cc", { default: "" });
const bcc = defineModel<string>("bcc", { default: "" });
const subject = defineModel<string>("subject", { default: "" });

const firstField = ref<InstanceType<typeof RecipientInput> | null>(null);

defineExpose({
  /** Focuses the To field (the composer's initial focus target). */
  focus: (): void => firstField.value?.focus(),
});
</script>

<template>
  <RecipientInput
    ref="firstField"
    v-model="to"
    field-id="compose-to"
    label="To"
  />
  <RecipientInput
    v-model="cc"
    field-id="compose-cc"
    label="Cc"
  />
  <RecipientInput
    v-model="bcc"
    field-id="compose-bcc"
    label="Bcc"
  />
  <div class="field">
    <label for="compose-subject">Subject</label>
    <input
      id="compose-subject"
      v-model="subject"
      type="text"
      autocomplete="off"
    >
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

.field input {
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
}

.field input:focus-visible {
  border-color: var(--color-accent);
}
</style>
