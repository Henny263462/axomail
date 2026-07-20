<script setup lang="ts">
/**
 * The address book's left pane: a search box, a "New contact" button, the
 * contact list, and a compact group manager (create / delete). Pure
 * presentation — it emits selection and group intents; the dialog owns state
 * and persistence.
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";

import type { ContactDto, ContactGroupDto } from "../../types/mail";

defineProps<{
  contacts: ContactDto[];
  groups: ContactGroupDto[];
  selectedId: number | null;
}>();

const emit = defineEmits<{
  select: [id: number];
  newContact: [];
  createGroup: [name: string];
  deleteGroup: [id: number];
}>();

const { t } = useI18n({ useScope: "global" });

const query = defineModel<string>("query", { default: "" });
const newGroupName = ref("");

function submitGroup(): void {
  const name = newGroupName.value.trim();
  if (name === "") return;
  emit("createGroup", name);
  newGroupName.value = "";
}
</script>

<template>
  <div class="list-pane">
    <label class="search">
      <span class="sr-only">{{ t("contacts.list.searchContacts") }}</span>
      <input
        v-model="query"
        type="search"
        :placeholder="t('contacts.list.searchContacts')"
        :aria-label="t('contacts.list.searchContacts')"
      >
    </label>
    <button
      type="button"
      class="new-button"
      @click="emit('newContact')"
    >
      {{ t("contacts.list.newContact") }}
    </button>

    <p
      v-if="contacts.length === 0"
      class="empty"
    >
      {{ t("contacts.list.empty") }}
    </p>
    <ul
      v-else
      class="contacts"
      :aria-label="t('contacts.list.contactsAria')"
    >
      <li
        v-for="contact in contacts"
        :key="contact.id"
      >
        <button
          type="button"
          class="contact"
          :class="{ selected: contact.id === selectedId }"
          :aria-current="contact.id === selectedId"
          @click="emit('select', contact.id)"
        >
          <span class="contact-name">{{ contact.displayName }}</span>
          <span class="contact-email">{{ contact.primaryEmail ?? "—" }}</span>
        </button>
      </li>
    </ul>

    <section class="groups-panel">
      <h3 class="groups-title">
        {{ t("contacts.list.groupsTitle") }}
      </h3>
      <ul
        v-if="groups.length > 0"
        class="group-list"
        :aria-label="t('contacts.list.groupsAria')"
      >
        <li
          v-for="group in groups"
          :key="group.id"
          class="group-item"
        >
          <span>{{ group.name }} ({{ group.memberCount }})</span>
          <button
            type="button"
            class="icon"
            :aria-label="t('contacts.list.deleteGroupAria', { name: group.name })"
            @click="emit('deleteGroup', group.id)"
          >
            ×
          </button>
        </li>
      </ul>
      <form
        class="group-create"
        @submit.prevent="submitGroup"
      >
        <input
          v-model="newGroupName"
          type="text"
          :placeholder="t('contacts.list.newGroupPlaceholder')"
          :aria-label="t('contacts.list.newGroupAria')"
        >
        <button
          type="submit"
          class="button"
          :disabled="newGroupName.trim() === ''"
        >
          {{ t("common.add") }}
        </button>
      </form>
    </section>
  </div>
</template>

<style scoped>
.list-pane {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  overflow-y: auto;
  max-height: 62vh;
}

.search input,
.group-create input {
  width: 100%;
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
}

.new-button {
  align-self: flex-start;
  font-size: var(--text-sm);
  color: var(--color-accent);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  padding: var(--space-1) var(--space-2);
  cursor: pointer;
}

.contacts {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.contact {
  display: flex;
  flex-direction: column;
  width: 100%;
  text-align: left;
  gap: 2px;
  padding: var(--space-1) var(--space-2);
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  cursor: pointer;
}

.contact:hover,
.contact:focus-visible {
  background: var(--color-bg-hover);
}

.contact.selected {
  border-color: var(--color-accent);
  background: var(--color-bg-hover);
}

.contact-name {
  font-size: var(--text-sm);
  color: var(--color-text);
}

.contact-email {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.empty {
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
}

.groups-panel {
  margin-top: auto;
  border-top: 1px solid var(--color-border);
  padding-top: var(--space-2);
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.groups-title {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--color-text-tertiary);
}

.group-list {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.group-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: var(--text-sm);
}

.group-create {
  display: flex;
  gap: var(--space-1);
}

.button {
  font-size: var(--text-sm);
  padding: var(--space-1) var(--space-3);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-strong);
  color: var(--color-text);
  background: var(--color-bg);
  cursor: pointer;
}

.button:hover {
  background: var(--color-bg-hover);
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

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
</style>
