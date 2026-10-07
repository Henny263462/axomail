<script setup lang="ts">
import AppIcon from "./AppIcon.vue";
import type { IconName } from "./icons";

defineProps<{
  title: string;
  detail?: string;
  icon?: IconName;
}>();
</script>

<template>
  <div class="empty">
    <span
      v-if="icon"
      class="empty-icon"
    >
      <AppIcon
        :name="icon"
        :size="26"
        :stroke-width="1.5"
      />
    </span>
    <p class="empty-title">
      {{ title }}
    </p>
    <p
      v-if="detail"
      class="empty-detail"
    >
      {{ detail }}
    </p>
  </div>
</template>

<style scoped>
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-1);
  height: 100%;
  padding: var(--space-6);
  text-align: center;
  animation: ax-rise-in var(--duration-slow) var(--ease-out);
}

.empty-icon {
  display: grid;
  place-items: center;
  width: 56px;
  height: 56px;
  margin-bottom: var(--space-2);
  border-radius: var(--radius-xl);
  background: var(--color-accent-subtle);
  color: var(--color-accent);
}

.empty-title {
  color: var(--color-text-secondary);
  font-weight: var(--weight-medium);
  font-size: var(--text-sm);
}

.empty-detail {
  color: var(--color-text-tertiary);
  font-size: var(--text-sm);
  max-width: 36ch;
}
</style>
