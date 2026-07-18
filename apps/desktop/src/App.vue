<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onMounted, ref } from "vue";

// Placeholder shell for work package 1a. The three-column layout and the
// design-token system follow in work package 1d.
const version = ref<string | null>(null);

onMounted(async () => {
  try {
    version.value = await invoke<string>("app_version");
  } catch {
    // Not running inside Tauri (e.g. plain `vite dev` or tests).
    version.value = null;
  }
});
</script>

<template>
  <main class="shell">
    <h1 class="title">
      axomail
    </h1>
    <p class="subtitle">
      A fast, lightweight, open-source desktop mail client.
    </p>
    <p
      v-if="version !== null"
      class="version"
    >
      version {{ version }}
    </p>
  </main>
</template>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 100vh;
  gap: 0.5rem;
}

.title {
  font-size: 1.5rem;
  font-weight: 600;
  letter-spacing: -0.02em;
}

.subtitle {
  color: var(--color-text-secondary);
}

.version {
  color: var(--color-text-tertiary);
  font-size: 0.8125rem;
}
</style>
