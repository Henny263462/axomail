<script setup lang="ts">
/**
 * The message-template picker: lists saved templates and starts a new message
 * from the chosen one (`select`), or deletes a template. Templates are created
 * from the composer's "Save as template". Opened by the `compose.newFromTemplate`
 * command (command palette) via the dialog host.
 */
import { onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import { deleteTemplate, listTemplates } from "../../api/templates";
import { useFocusTrap } from "../../composables/useFocusTrap";
import { isTauri } from "../../lib/tauri";
import type { MessageTemplateDto } from "../../types/templates";

const emit = defineEmits<{
  close: [];
  select: [template: MessageTemplateDto];
}>();

const { t } = useI18n({ useScope: "global" });

const templates = ref<MessageTemplateDto[]>([]);
const loaded = ref(false);
const unavailable = ref(false);

const root = ref<HTMLElement | null>(null);
/** The control that had focus before the dialog opened, restored on close. */
let invoker: HTMLElement | null = null;
const { onKeydown } = useFocusTrap(root, close);

onMounted(() => {
  // Move focus into the modal so keyboard users are not left behind it, then
  // load the list.
  invoker = document.activeElement as HTMLElement | null;
  root.value
    ?.querySelector<HTMLElement>(
      "button, [href], input, select, textarea, [tabindex]",
    )
    ?.focus();
  void load();
});
// Return focus to the invoking control when the dialog closes.
onBeforeUnmount(() => invoker?.focus());

async function load(): Promise<void> {
  if (!isTauri()) {
    unavailable.value = true;
    loaded.value = true;
    return;
  }
  const list = await listTemplates();
  if (list !== null) templates.value = list;
  loaded.value = true;
}

function choose(template: MessageTemplateDto): void {
  emit("select", template);
}

async function remove(template: MessageTemplateDto): Promise<void> {
  await deleteTemplate(template.id);
  templates.value = templates.value.filter((item) => item.id !== template.id);
}

function close(): void {
  emit("close");
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
      aria-labelledby="template-picker-title"
    >
      <div class="head">
        <h2
          id="template-picker-title"
          class="title"
        >
          {{ t("templates.title") }}
        </h2>
        <button
          type="button"
          class="close"
          :aria-label="t('templates.close')"
          @click="close"
        >
          ×
        </button>
      </div>

      <p
        v-if="unavailable"
        class="empty"
      >
        {{ t("templates.unavailable") }}
      </p>
      <p
        v-else-if="loaded && templates.length === 0"
        class="empty"
      >
        {{ t("templates.empty") }}
      </p>
      <ul
        v-else
        class="list"
      >
        <li
          v-for="template in templates"
          :key="template.id"
          class="row"
        >
          <span class="info">
            <span class="name">{{ template.name }}</span>
            <span
              v-if="template.subject"
              class="subject"
            >{{ template.subject }}</span>
          </span>
          <span class="row-actions">
            <button
              type="button"
              class="row-action"
              @click="choose(template)"
            >
              {{ t("templates.new") }}
            </button>
            <button
              type="button"
              class="row-action ghost"
              @click="remove(template)"
            >
              {{ t("templates.delete") }}
            </button>
          </span>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: var(--space-6) var(--space-4);
  overflow-y: auto;
  background: rgb(0 0 0 / 40%);
}

.dialog {
  width: 100%;
  max-width: 440px;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
}

.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: var(--space-3);
}

.title {
  font-size: var(--text-lg);
  font-weight: var(--weight-semibold);
}

.close {
  font-size: var(--text-lg);
  line-height: 1;
  color: var(--color-text-tertiary);
  background: none;
  border: none;
  cursor: pointer;
}

.empty {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  list-style: none;
}

.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--space-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
}

.info {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.name {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text);
}

.subject {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-actions {
  display: inline-flex;
  gap: var(--space-2);
  flex-shrink: 0;
}

.row-action {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  font-size: var(--text-xs);
  color: var(--color-accent);
  cursor: pointer;
}

.row-action.ghost {
  color: var(--color-text-secondary);
}

.row-action:hover {
  background: var(--color-bg-hover);
}
</style>
