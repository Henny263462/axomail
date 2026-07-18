<script setup lang="ts">
defineProps<{
  title: string;
  detail?: string;
}>();
</script>

<template>
  <div class="empty">
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
