<script setup lang="ts">
/**
 * Accessible search field for the message-list pane. Owns no search state —
 * it binds a query string and reports Escape so the host can restore the
 * mailbox view. Debouncing and requests live in the host's `useSearch`.
 */
import { useI18n } from "vue-i18n";

const { t } = useI18n({ useScope: "global" });

const model = defineModel<string>({ default: "" });

const emit = defineEmits<{
  escape: [];
}>();

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    emit("escape");
  }
}
</script>

<template>
  <div class="search">
    <input
      class="input"
      type="search"
      role="searchbox"
      :aria-label="t('list.search.searchMessages')"
      :placeholder="t('list.search.searchMessages')"
      autocomplete="off"
      :value="model"
      @input="model = ($event.target as HTMLInputElement).value"
      @keydown="onKeydown"
    >
  </div>
</template>

<style scoped>
.search {
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg);
}

.input {
  width: 100%;
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg-subtle);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  padding: var(--space-1) var(--space-2);
}

.input:focus-visible {
  border-color: var(--color-accent);
  background: var(--color-bg);
}
</style>
