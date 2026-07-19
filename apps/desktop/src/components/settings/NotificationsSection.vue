<script setup lang="ts">
import type { SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

function onToggle(event: Event): void {
  props.context.setNotificationsEnabled(
    (event.target as HTMLInputElement).checked,
  );
}
</script>

<template>
  <label class="toggle">
    <input
      id="settings-notifications"
      type="checkbox"
      :checked="context.notificationsEnabled"
      @change="onToggle"
    >
    <span>Show desktop notifications for new mail</span>
  </label>
</template>

<style scoped>
.toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
}
</style>
