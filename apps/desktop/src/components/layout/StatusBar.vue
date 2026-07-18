<script setup lang="ts">
import { useAppVersion } from "../../composables/useAppVersion";
import { isTauri } from "../../lib/tauri";

withDefaults(
  defineProps<{
    /** Redacted sync failure to surface, or null when there is none. */
    error?: string | null;
  }>(),
  { error: null },
);

const emit = defineEmits<{
  dismissError: [];
}>();

const { version } = useAppVersion();
const browserPreview = !isTauri();
</script>

<template>
  <footer
    class="status"
    aria-label="Status"
  >
    <span class="app">
      axomail<template v-if="version !== null"> {{ version }}</template>
    </span>
    <span
      v-if="error"
      class="error"
      role="alert"
    >
      {{ error }}
      <button
        type="button"
        class="dismiss"
        aria-label="Dismiss error"
        @click="emit('dismissError')"
      >
        ✕
      </button>
    </span>
    <span
      v-else-if="browserPreview"
      class="mode"
    >
      Browser preview — mail features require the desktop app
    </span>
  </footer>
</template>

<style scoped>
.status {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: 0 var(--space-3);
  height: 28px;
  border-top: 1px solid var(--color-border);
  background: var(--color-bg-subtle);
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.mode {
  color: var(--color-text-secondary);
}

.error {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--color-danger);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dismiss {
  flex-shrink: 0;
  font-size: var(--text-xs);
  line-height: 1;
  color: var(--color-danger);
  padding: 2px var(--space-1);
  border-radius: var(--radius-sm);
}

.dismiss:hover {
  background: var(--color-bg-hover);
}
</style>
