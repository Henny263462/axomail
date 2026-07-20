<script setup lang="ts">
/**
 * Keyboard-shortcut help overlay (opened with <kbd>?</kbd>): a modal that lists
 * every bound shortcut, grouped by area, generated from the command registry.
 * Read-only — it is a reference card, not an editor.
 */
import { computed, nextTick, onMounted, ref } from "vue";

import { shortcutGroups } from "../../commands/display";
import { useCommandRegistry } from "../../composables/useCommandRegistry";
import { useFocusTrap } from "../../composables/useFocusTrap";
import ShortcutList from "./ShortcutList.vue";

const emit = defineEmits<{ close: [] }>();

const registry = useCommandRegistry();
const root = ref<HTMLElement | null>(null);
const closeButton = ref<HTMLButtonElement | null>(null);
const { onKeydown } = useFocusTrap(root, () => emit("close"));

const groups = computed(() => shortcutGroups(registry.commands.value));

// Move focus into the overlay so Escape / Tab reach the trap (the overlay has
// no autofocusing input to pull focus in like the palette does).
onMounted(() => {
  void nextTick(() => closeButton.value?.focus());
});
</script>

<template>
  <div
    class="overlay"
    @keydown="onKeydown"
    @click.self="emit('close')"
  >
    <div
      ref="root"
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="shortcut-help-title"
    >
      <div class="header">
        <h2
          id="shortcut-help-title"
          class="title"
        >
          Keyboard shortcuts
        </h2>
        <button
          ref="closeButton"
          type="button"
          class="close"
          aria-label="Close"
          @click="emit('close')"
        >
          ✕
        </button>
      </div>
      <ShortcutList :groups="groups" />
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 150;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: var(--space-6) var(--space-4);
  overflow-y: auto;
  background: rgb(0 0 0 / 40%);
}

.dialog {
  width: 100%;
  max-width: 460px;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
}

.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: var(--space-4);
}

.title {
  font-size: var(--text-lg);
  font-weight: var(--weight-semibold);
  letter-spacing: -0.01em;
}

.close {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-md);
}

.close:hover {
  background: var(--color-bg-hover);
}
</style>
