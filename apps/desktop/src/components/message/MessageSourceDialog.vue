<script setup lang="ts">
/**
 * Read-only "View source" modal: shows the raw RFC 5322 `.eml` text of a
 * message in a monospace, non-editable pane. The content is plain text placed
 * in a `<pre>` (never parsed as markup), so nothing in the message can execute
 * here. Escape or the Close button dismisses it; focus is trapped while open.
 */
import { nextTick, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

const { t } = useI18n({ useScope: "global" });

defineProps<{
  /** Dialog heading (e.g. the message subject). */
  title: string;
  /** Raw source text to display, or null while it is still loading. */
  source: string | null;
}>();

const emit = defineEmits<{ close: [] }>();

const root = ref<HTMLElement | null>(null);

function close(): void {
  emit("close");
}

function focusables(): HTMLElement[] {
  if (root.value === null) return [];
  return Array.from(
    root.value.querySelectorAll<HTMLElement>("button:not([disabled])"),
  );
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    close();
    return;
  }
  if (event.key !== "Tab") return;
  const items = focusables();
  if (items.length === 0) return;
  const first = items[0];
  const last = items[items.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
}

onMounted(async () => {
  await nextTick();
  focusables()[0]?.focus();
});
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
      aria-labelledby="view-source-title"
    >
      <header class="head">
        <h2
          id="view-source-title"
          class="title"
        >
          {{ t("message.source.heading", { title }) }}
        </h2>
      </header>

      <p
        v-if="source === null"
        class="note"
        role="status"
      >
        {{ t("message.source.loading") }}
      </p>
      <pre
        v-else
        class="source"
        tabindex="0"
        :aria-label="t('message.source.rawLabel')"
      >{{ source }}</pre>

      <div class="actions">
        <button
          type="button"
          class="button"
          @click="close"
        >
          {{ t("common.close") }}
        </button>
      </div>
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
  display: flex;
  flex-direction: column;
  width: 100%;
  max-width: 760px;
  max-height: 85vh;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
}

.head {
  margin-bottom: var(--space-3);
}

.title {
  font-size: var(--text-lg);
  font-weight: var(--weight-semibold);
  letter-spacing: -0.01em;
  overflow-wrap: anywhere;
}

.note {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.source {
  flex: 1;
  overflow: auto;
  margin: 0;
  padding: var(--space-3);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-bg-subtle);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  line-height: 1.5;
  color: var(--color-text);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.actions {
  display: flex;
  justify-content: flex-end;
  margin-top: var(--space-4);
}

.button {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-strong);
  color: var(--color-text);
  background: var(--color-bg);
  transition: background var(--transition-fast);
}

.button:hover {
  background: var(--color-bg-hover);
}
</style>
