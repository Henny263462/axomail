<script setup lang="ts">
/**
 * Reading-pane action bar: save the whole message as `.eml`, view its raw
 * source, and toggle the full RFC 5322 header block. Bytes and source are
 * served by Rust commands; the save destination comes from the native dialog
 * (ADR-0010). Header/source text is fetched lazily and reset when the open
 * message changes. Inert in the browser preview, where the bridge is absent.
 */
import { ref, watch } from "vue";

import {
  getMessageHeaders,
  getMessageSource,
  pickSaveFilePath,
  saveMessageEml,
} from "../../api/attachments";
import MessageSourceDialog from "./MessageSourceDialog.vue";

const props = defineProps<{
  messageId: number;
  subject: string | null;
}>();

const emit = defineEmits<{
  /** Print the open message (a print-friendly, sanitized document). */
  print: [];
}>();

const headersOpen = ref(false);
const headers = ref<string | null>(null);
const sourceOpen = ref(false);
const source = ref<string | null>(null);
const status = ref<string | null>(null);

// Reset cached, per-message state whenever the open message changes so a
// toggle never shows a previous message's headers or source.
watch(
  () => props.messageId,
  () => {
    headersOpen.value = false;
    headers.value = null;
    sourceOpen.value = false;
    source.value = null;
    status.value = null;
  },
);

/** A dialog-friendly default `.eml` name derived from the subject. */
function emlName(): string {
  const base = (props.subject ?? "message")
    .replace(/[\\/:*?"<>|]/g, "_")
    .trim();
  return `${base || "message"}.eml`;
}

async function onSaveEml(): Promise<void> {
  const dest = await pickSaveFilePath(emlName(), [
    { name: "Email message", extensions: ["eml"] },
  ]);
  if (dest === null) return;
  status.value = null;
  try {
    const bytes = await saveMessageEml(props.messageId, dest);
    status.value = bytes === null ? null : "Saved message as .eml.";
  } catch {
    status.value = "Could not save the message.";
  }
}

async function onViewSource(): Promise<void> {
  sourceOpen.value = true;
  if (source.value === null) {
    source.value = (await getMessageSource(props.messageId)) ?? "";
  }
}

async function onToggleHeaders(): Promise<void> {
  headersOpen.value = !headersOpen.value;
  if (headersOpen.value && headers.value === null) {
    headers.value = (await getMessageHeaders(props.messageId)) ?? "";
  }
}
</script>

<template>
  <div class="actions-area">
    <div
      class="bar"
      aria-label="Message actions"
    >
      <button
        type="button"
        class="action"
        @click="onSaveEml"
      >
        Save as .eml
      </button>
      <button
        type="button"
        class="action"
        @click="onViewSource"
      >
        View source
      </button>
      <button
        type="button"
        class="action"
        :aria-pressed="headersOpen"
        @click="onToggleHeaders"
      >
        {{ headersOpen ? "Hide headers" : "All headers" }}
      </button>
      <button
        type="button"
        class="action"
        @click="emit('print')"
      >
        Print
      </button>
    </div>

    <pre
      v-if="headersOpen"
      class="headers"
      aria-label="Full message headers"
    >{{ headers ?? "Loading headers…" }}</pre>

    <p
      v-if="status !== null"
      class="status"
      role="status"
      aria-live="polite"
    >
      {{ status }}
    </p>

    <MessageSourceDialog
      v-if="sourceOpen"
      :title="subject ?? '(no subject)'"
      :source="source"
      @close="sourceOpen = false"
    />
  </div>
</template>

<style scoped>
.actions-area {
  margin-top: var(--space-3);
}

.bar {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.action {
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: background var(--transition-fast);
}

.action:hover {
  background: var(--color-bg-hover);
}

.action[aria-pressed="true"] {
  color: var(--color-accent);
  border-color: var(--color-accent);
}

.headers {
  margin: var(--space-2) 0 0;
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-bg-subtle);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  line-height: 1.5;
  color: var(--color-text-secondary);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  max-height: 260px;
  overflow: auto;
}

.status {
  margin-top: var(--space-2);
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}
</style>
