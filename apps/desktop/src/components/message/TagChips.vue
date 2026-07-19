<script setup lang="ts">
/**
 * Renders a message's tags as compact chips (a colored dot plus the label).
 * Shared by the list row and the detail header. Purely presentational; the
 * color is a backend-validated `#rrggbb` value, so binding it to an inline
 * style is safe.
 */
import type { TagChip } from "../../types/mail";

withDefaults(
  defineProps<{
    tags: TagChip[];
    /** Accessible label for the chip group. */
    label?: string;
  }>(),
  { label: "Tags" },
);

function dotStyle(color: string | null): Record<string, string> {
  return color === null
    ? { border: "1px solid var(--color-border-strong)" }
    : { backgroundColor: color };
}
</script>

<template>
  <ul
    v-if="tags.length > 0"
    class="chips"
    :aria-label="label"
  >
    <li
      v-for="tag in tags"
      :key="tag.id"
      class="chip"
    >
      <span
        class="dot"
        :style="dotStyle(tag.color)"
        aria-hidden="true"
      />
      <span class="name">{{ tag.displayName }}</span>
    </li>
  </ul>
</template>

<style scoped>
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1) var(--space-2);
  align-items: center;
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: 1px var(--space-2) 1px var(--space-1);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-pill, 999px);
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  background: var(--color-bg-subtle);
  max-width: 100%;
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
