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
import { useI18n } from "vue-i18n";

const emit = defineEmits<{
  command: [name: string];
  link: [url: string];
}>();

const { t } = useI18n({ useScope: "global" });

/** Simple formatting buttons: [command name, label key, aria label key]. */
const BUTTONS: ReadonlyArray<readonly [string, string, string]> = [
  ["bold", "compose.toolbar.boldLabel", "compose.toolbar.boldAria"],
  ["italic", "compose.toolbar.italicLabel", "compose.toolbar.italicAria"],
  ["underline", "compose.toolbar.underlineLabel", "compose.toolbar.underlineAria"],
  ["insertUnorderedList", "compose.toolbar.bulletedLabel", "compose.toolbar.bulletedAria"],
  ["insertOrderedList", "compose.toolbar.numberedLabel", "compose.toolbar.numberedAria"],
  ["formatBlock:blockquote", "compose.toolbar.quoteLabel", "compose.toolbar.quoteAria"],
  ["removeFormat", "compose.toolbar.removeFormatLabel", "compose.toolbar.removeFormatAria"],
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
    :aria-label="t('compose.toolbar.formatting')"
  >
    <button
      v-for="[name, labelKey, ariaKey] in BUTTONS"
      :key="name"
      type="button"
      class="tool"
      :aria-label="t(ariaKey)"
      :title="t(ariaKey)"
      @mousedown.prevent
      @click="run(name)"
    >
      {{ t(labelKey) }}
    </button>
    <button
      type="button"
      class="tool"
      :aria-label="t('compose.toolbar.linkAria')"
      :title="t('compose.toolbar.linkAria')"
      :aria-expanded="linkOpen"
      @mousedown.prevent
      @click="toggleLink"
    >
      {{ t("compose.toolbar.linkLabel") }}
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
        :placeholder="t('compose.toolbar.linkUrlPlaceholder')"
        :aria-label="t('compose.toolbar.linkUrlAria')"
        @keydown.enter.prevent="confirmLink"
        @keydown.esc.prevent="linkOpen = false"
      >
      <button
        type="button"
        class="tool"
        @click="confirmLink"
      >
        {{ t("compose.toolbar.insert") }}
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
