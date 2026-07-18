<script setup lang="ts">
import { useAppVersion } from "../../composables/useAppVersion";
import { isTauri } from "../../lib/tauri";

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
      v-if="browserPreview"
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
</style>
