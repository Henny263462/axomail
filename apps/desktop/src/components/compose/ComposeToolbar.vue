<script setup lang="ts">
/**
 * The rich-editor formatting toolbar: bold / italic / underline, bulleted and
 * numbered lists, blockquote, remove-formatting, and an insert-link affordance.
 *
 * The toolbar owns no content — it only emits the user's intent. `command`
 * carries a formatting command name the parent runs against its contenteditable
 * surface; `link` carries a URL to turn the selection into a hyperlink. Keeping
 * the DOM commands in the parent means this component is pure UI and the
 * produced HTML is always funnelled through the parent's sanitizer.
 */
import { nextTick, ref } from "vue";

const emit = defineEmits<{
  command: [name: string];
  link: [url: string];
}>();

/** Simple formatting buttons: [command name, visible label, aria label]. */
const BUTTONS: ReadonlyArray<readonly [string, string, string]> = [
  ["bold", "B", "Bold"],
  ["italic", "I", "Italic"],
  ["underline", "U", "Underline"],
  ["insertUnorderedList", "• List", "Bulleted list"],
  ["insertOrderedList", "1. List", "Numbered list"],
  ["formatBlock:blockquote", "❝ Quote", "Blockquote"],
  ["removeFormat", "⨯ Format", "Remove formatting"],
];

const linkOpen = ref(false);
const linkUrl = ref("");
const linkInput = ref<HTMLInputElement | null>(null);

function run(name: string): void {
  emit("command", name);
}

function toggleLink(): void {
  linkOpen.value = !linkOpen.value;
  if (linkOpen.value) {
    void nextTick(() => linkInput.value?.focus());
  }
}

function confirmLink(): void {
  const url = linkUrl.value.trim();
  if (url.length > 0) emit("link", url);
  linkUrl.value = "";
  linkOpen.value = false;
}
</script>

<template>
  <div
    class="toolbar"
    role="toolbar"
    aria-label="Text formatting"
  >
    <button
      v-for="[name, label, aria] in BUTTONS"
      :key="name"
      type="button"
      class="tool"
      :aria-label="aria"
      :title="aria"
      @mousedown.prevent
      @click="run(name)"
    >
      {{ label }}
    </button>
    <button
      type="button"
      class="tool"
      aria-label="Insert link"
      title="Insert link"
      :aria-expanded="linkOpen"
      @mousedown.prevent
      @click="toggleLink"
    >
      🔗 Link
    </button>

    <div
      v-if="linkOpen"
      class="link-row"
    >
      <input
        id="compose-link-url"
        ref="linkInput"
        v-model="linkUrl"
        type="url"
        placeholder="https://example.com"
        aria-label="Link URL"
        @keydown.enter.prevent="confirmLink"
        @keydown.esc.prevent="linkOpen = false"
      >
      <button
        type="button"
        class="tool"
        @click="confirmLink"
      >
        Insert
      </button>
    </div>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-1);
  border: 1px solid var(--color-border-strong);
  border-bottom: none;
  border-radius: var(--radius-sm) var(--radius-sm) 0 0;
  background: var(--color-bg-subtle);
}

.tool {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
  cursor: pointer;
}

.tool:hover {
  background: var(--color-bg-hover);
}

.link-row {
  display: flex;
  gap: var(--space-1);
  flex-basis: 100%;
  margin-top: var(--space-1);
}

.link-row input {
  flex: 1;
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
}
</style>
