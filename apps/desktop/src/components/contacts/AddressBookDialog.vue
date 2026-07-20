<script setup lang="ts">
/**
 * The address book: a two-pane dialog listing contacts (searchable, with a
 * compact group manager) beside an editor for the selected or new contact, with
 * vCard import/export. Persistence goes through the contact api; the dialog
 * reloads its own list after each change. Focus is trapped and Escape closes
 * (shared {@link useFocusTrap}). Honest empty states and aria labels throughout.
 */
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import {
  createContactGroup,
  createContact,
  deleteContact,
  deleteContactGroup,
  exportContactsVcard,
  getContact,
  importContactsVcard,
  listContactGroups,
  listContacts,
  setContactGroups,
  updateContact,
} from "../../api/contacts";
import { useFocusTrap } from "../../composables/useFocusTrap";
import { isTauri } from "../../lib/tauri";
import type {
  ContactDetailDto,
  ContactDto,
  ContactGroupDto,
  ContactInput,
} from "../../types/mail";
import ContactEditor from "./ContactEditor.vue";
import ContactList from "./ContactList.vue";

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n({ useScope: "global" });

const root = ref<HTMLElement | null>(null);
let invoker: HTMLElement | null = null;
const { onKeydown } = useFocusTrap(root, () => emit("close"));

const contacts = ref<ContactDto[]>([]);
const groups = ref<ContactGroupDto[]>([]);
const query = ref("");
const selectedId = ref<number | null>(null);
const editing = ref<ContactDetailDto | null>(null);
const editorKey = ref(0);
const showEditor = ref(false);
const status = ref("");

let searchTimer: ReturnType<typeof setTimeout> | null = null;

onMounted(() => {
  invoker = document.activeElement as HTMLElement | null;
  void load();
  void loadGroups();
  // Move focus into the dialog so its keydown trap receives events (Escape to
  // close, Tab to cycle). Without this, focus stays on the invoker outside the
  // overlay and the handler never fires.
  void nextTick(() => {
    const first = root.value?.querySelector<HTMLElement>(
      "button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex='-1'])",
    );
    first?.focus();
  });
});

onBeforeUnmount(() => {
  if (searchTimer !== null) clearTimeout(searchTimer);
  invoker?.focus();
});

watch(query, () => {
  if (searchTimer !== null) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => void load(), 150);
});

async function load(): Promise<void> {
  contacts.value = (await listContacts(query.value)) ?? [];
}

async function loadGroups(): Promise<void> {
  groups.value = (await listContactGroups()) ?? [];
}

function guardDesktop(): boolean {
  if (isTauri()) return true;
  status.value = t("contacts.dialog.desktopOnly");
  return false;
}

async function select(id: number): Promise<void> {
  selectedId.value = id;
  editing.value = await getContact(id);
  editorKey.value += 1;
  showEditor.value = true;
}

function startNew(): void {
  selectedId.value = null;
  editing.value = null;
  editorKey.value += 1;
  showEditor.value = true;
  status.value = "";
}

async function onSave(
  input: ContactInput,
  groupIds: number[],
  contactId: number | null,
): Promise<void> {
  if (!guardDesktop()) return;
  try {
    const saved =
      contactId === null
        ? await createContact(input)
        : await updateContact(contactId, input);
    if (saved === null) return;
    await setContactGroups(saved.id, groupIds);
    status.value = t("contacts.dialog.saved", { name: saved.displayName });
    await load();
    await loadGroups();
    await select(saved.id);
  } catch (error) {
    status.value = messageFrom(error);
  }
}

async function onDelete(contactId: number): Promise<void> {
  if (!guardDesktop()) return;
  try {
    await deleteContact(contactId);
    status.value = t("contacts.dialog.deleted");
    showEditor.value = false;
    selectedId.value = null;
    await load();
    await loadGroups();
  } catch (error) {
    status.value = messageFrom(error);
  }
}

async function onImport(): Promise<void> {
  if (!guardDesktop()) return;
  const outcome = await importContactsVcard();
  if (outcome === null) return;
  status.value = t("contacts.dialog.imported", {
    imported: outcome.imported,
    skipped: outcome.skipped,
  });
  await load();
  await loadGroups();
}

async function onExport(): Promise<void> {
  if (!guardDesktop()) return;
  const count = await exportContactsVcard("3.0");
  if (count === null) return;
  status.value = t("contacts.dialog.exported", { count });
}

async function onCreateGroup(name: string): Promise<void> {
  if (!guardDesktop()) return;
  try {
    await createContactGroup(name);
    await loadGroups();
  } catch (error) {
    status.value = messageFrom(error);
  }
}

async function onDeleteGroup(id: number): Promise<void> {
  if (!guardDesktop()) return;
  await deleteContactGroup(id);
  await loadGroups();
  if (selectedId.value !== null) await select(selectedId.value);
}

function messageFrom(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
</script>

<template>
  <div
    class="overlay"
    @keydown="onKeydown"
  >
    <div
      ref="root"
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="address-book-title"
    >
      <header class="head">
        <h2
          id="address-book-title"
          class="title"
        >
          {{ t("contacts.dialog.title") }}
        </h2>
        <div class="head-actions">
          <button
            type="button"
            class="button"
            @click="onImport"
          >
            {{ t("contacts.dialog.importVcard") }}
          </button>
          <button
            type="button"
            class="button"
            @click="onExport"
          >
            {{ t("contacts.dialog.exportVcard") }}
          </button>
          <button
            type="button"
            class="button"
            :aria-label="t('contacts.dialog.closeAria')"
            @click="emit('close')"
          >
            {{ t("common.close") }}
          </button>
        </div>
      </header>

      <p
        v-if="status"
        class="status"
        role="status"
      >
        {{ status }}
      </p>

      <div class="body">
        <ContactList
          v-model:query="query"
          :contacts="contacts"
          :groups="groups"
          :selected-id="selectedId"
          @select="select"
          @new-contact="startNew"
          @create-group="onCreateGroup"
          @delete-group="onDeleteGroup"
        />

        <div class="editor-pane">
          <ContactEditor
            v-if="showEditor"
            :key="editorKey"
            :contact="editing"
            :groups="groups"
            @save="onSave"
            @delete-contact="onDelete"
          />
          <p
            v-else
            class="empty hint"
          >
            {{ t("contacts.dialog.empty") }}
          </p>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 120;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: var(--space-6) var(--space-4);
  overflow-y: auto;
  background: rgb(0 0 0 / 40%);
}

.dialog {
  width: 100%;
  max-width: 780px;
  max-height: 86vh;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
}

.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}

.title {
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
}

.head-actions {
  display: flex;
  gap: var(--space-2);
}

.status {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.body {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.2fr);
  gap: var(--space-4);
  overflow: hidden;
}

.editor-pane {
  overflow-y: auto;
  max-height: 62vh;
  padding-right: var(--space-1);
}

.empty {
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
}

.hint {
  padding: var(--space-4) 0;
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
</style>
