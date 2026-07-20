<script setup lang="ts">
/**
 * The add/edit form for one contact: name, note, a repeatable list of addresses
 * (each with a label and a "primary" radio), and group-membership checkboxes.
 * A `null` contact is the "new contact" form. The parent owns persistence — this
 * component validates locally and emits the {@link ContactInput} plus the chosen
 * group ids on save.
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import type {
  ContactDetailDto,
  ContactGroupDto,
  ContactInput,
} from "../../types/mail";

interface EmailRow {
  email: string;
  label: string;
}

const props = defineProps<{
  /** The contact to edit, or `null` for a new contact. */
  contact: ContactDetailDto | null;
  /** All groups, for membership checkboxes. */
  groups: ContactGroupDto[];
}>();

const emit = defineEmits<{
  save: [input: ContactInput, groupIds: number[], contactId: number | null];
  deleteContact: [contactId: number];
}>();

const { t } = useI18n({ useScope: "global" });

const displayName = ref("");
const note = ref("");
const rows = ref<EmailRow[]>([]);
const primaryIndex = ref(0);
const selectedGroups = ref<Set<number>>(new Set());
const error = ref("");
const confirmingDelete = ref(false);

watch(() => props.contact, reset, { immediate: true });

function reset(): void {
  error.value = "";
  confirmingDelete.value = false;
  const contact = props.contact;
  displayName.value = contact?.displayName ?? "";
  note.value = contact?.note ?? "";
  rows.value =
    contact && contact.emails.length > 0
      ? contact.emails.map((e) => ({ email: e.email, label: e.label }))
      : [{ email: "", label: "" }];
  const primary = contact?.emails.findIndex((e) => e.isPrimary) ?? -1;
  primaryIndex.value = primary >= 0 ? primary : 0;
  selectedGroups.value = new Set(contact?.groupIds ?? []);
}

const isNew = computed(() => props.contact === null);

function addRow(): void {
  rows.value.push({ email: "", label: "" });
}

function removeRow(index: number): void {
  rows.value.splice(index, 1);
  if (rows.value.length === 0) rows.value.push({ email: "", label: "" });
  if (primaryIndex.value >= rows.value.length) primaryIndex.value = 0;
}

function toggleGroup(id: number, checked: boolean): void {
  const next = new Set(selectedGroups.value);
  if (checked) next.add(id);
  else next.delete(id);
  selectedGroups.value = next;
}

function submit(): void {
  error.value = "";
  const emails = rows.value
    .map((row, index) => ({
      email: row.email.trim(),
      label: row.label.trim(),
      isPrimary: index === primaryIndex.value,
    }))
    .filter((row) => row.email !== "");
  if (displayName.value.trim() === "" && emails.length === 0) {
    error.value = t("contacts.editor.errorEmpty");
    return;
  }
  // Ensure exactly one primary survives the empty-row filter.
  const first = emails[0];
  if (first !== undefined && !emails.some((e) => e.isPrimary)) {
    first.isPrimary = true;
  }
  const input: ContactInput = {
    displayName: displayName.value.trim(),
    emails,
    note: note.value.trim(),
  };
  emit("save", input, [...selectedGroups.value], props.contact?.id ?? null);
}

function requestDelete(): void {
  if (props.contact === null) return;
  if (!confirmingDelete.value) {
    confirmingDelete.value = true;
    return;
  }
  emit("deleteContact", props.contact.id);
}
</script>

<template>
  <form
    class="editor"
    :aria-label="isNew ? t('contacts.editor.newContactAria') : t('contacts.editor.editAria', { name: contact?.displayName })"
    @submit.prevent="submit"
  >
    <div class="field">
      <label for="contact-name">{{ t("common.name") }}</label>
      <input
        id="contact-name"
        v-model="displayName"
        type="text"
        autocomplete="off"
      >
    </div>

    <fieldset class="emails">
      <legend>{{ t("contacts.editor.emailAddresses") }}</legend>
      <div
        v-for="(row, index) in rows"
        :key="index"
        class="email-row"
      >
        <input
          v-model="row.email"
          type="email"
          class="email"
          :aria-label="t('contacts.editor.emailAddressAria', { n: index + 1 })"
          :placeholder="t('contacts.editor.emailPlaceholder')"
        >
        <input
          v-model="row.label"
          type="text"
          class="label-input"
          :aria-label="t('contacts.editor.labelAria', { n: index + 1 })"
          :placeholder="t('contacts.editor.labelPlaceholder')"
        >
        <label class="primary">
          <input
            v-model="primaryIndex"
            type="radio"
            name="primary"
            :value="index"
          >
          {{ t("contacts.editor.primary") }}
        </label>
        <button
          type="button"
          class="icon"
          :aria-label="t('contacts.editor.removeAddressAria', { n: index + 1 })"
          @click="removeRow(index)"
        >
          ×
        </button>
      </div>
      <button
        type="button"
        class="link"
        @click="addRow"
      >
        {{ t("contacts.editor.addAddress") }}
      </button>
    </fieldset>

    <div class="field">
      <label for="contact-note">{{ t("contacts.editor.note") }}</label>
      <textarea
        id="contact-note"
        v-model="note"
        rows="2"
      />
    </div>

    <fieldset
      v-if="groups.length > 0"
      class="groups"
    >
      <legend>{{ t("contacts.editor.groups") }}</legend>
      <label
        v-for="group in groups"
        :key="group.id"
        class="group-check"
      >
        <input
          type="checkbox"
          :checked="selectedGroups.has(group.id)"
          @change="toggleGroup(group.id, ($event.target as HTMLInputElement).checked)"
        >
        {{ group.name }}
      </label>
    </fieldset>

    <p
      v-if="error"
      class="error"
      role="alert"
    >
      {{ error }}
    </p>

    <div class="actions">
      <button
        type="submit"
        class="primary-button"
      >
        {{ isNew ? t("contacts.editor.createContact") : t("contacts.editor.saveChanges") }}
      </button>
      <template v-if="!isNew">
        <span
          v-if="confirmingDelete"
          class="confirm"
        >{{ t("contacts.editor.confirmDelete") }}</span>
        <button
          type="button"
          class="danger"
          @click="requestDelete"
        >
          {{ confirmingDelete ? t("contacts.editor.confirmDeleteButton") : t("common.delete") }}
        </button>
      </template>
    </div>
  </form>
</template>

<style scoped>
.editor {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.field label,
legend {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

input,
textarea {
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
}

.emails,
.groups {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-2);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.email-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
}

.email {
  flex: 2;
  min-width: 10rem;
}

.label-input {
  flex: 1;
  min-width: 5rem;
}

.primary {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.icon {
  border: none;
  background: none;
  color: var(--color-text-secondary);
  cursor: pointer;
  font-size: var(--text-md);
  line-height: 1;
}

.icon:hover {
  color: var(--color-danger);
}

.group-check {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  font-size: var(--text-sm);
}

.link {
  align-self: flex-start;
  border: none;
  background: none;
  color: var(--color-accent);
  cursor: pointer;
  font-size: var(--text-xs);
  padding: 0;
}

.error {
  font-size: var(--text-xs);
  color: var(--color-danger);
}

.actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.primary-button {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-accent);
  background: var(--color-accent);
  color: var(--color-accent-contrast, #fff);
  cursor: pointer;
}

.danger {
  font-size: var(--text-sm);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-danger);
  color: var(--color-danger);
  background: var(--color-bg);
  cursor: pointer;
}

.confirm {
  font-size: var(--text-xs);
  color: var(--color-danger);
}
</style>
