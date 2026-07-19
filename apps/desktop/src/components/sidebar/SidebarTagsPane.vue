<script setup lang="ts">
/**
 * The sidebar "Tags" group: a flat, cross-account list of tags. Clicking one
 * filters the message list to that tag (a frontend view over the
 * `list_messages_for_tag` command). Deliberately a SEPARATE child component
 * with its own data load, mounted at a marked point in {@link SidebarPane}, so
 * it never collides with the folder-tree rewrite that owns that file.
 */
import { onMounted, ref } from "vue";

import { listAllTags } from "../../api/tags";
import type { TagDto } from "../../types/mail";

defineProps<{
  /** Currently active tag filter, for highlighting. */
  activeTagId?: number | null;
}>();

const emit = defineEmits<{
  selectTag: [tagId: number, name: string];
}>();

const tags = ref<TagDto[]>([]);

onMounted(async () => {
  tags.value = (await listAllTags()) ?? [];
});

function dotStyle(color: string | null): Record<string, string> {
  return color === null
    ? { border: "1px solid var(--color-border-strong)" }
    : { backgroundColor: color };
}
</script>

<template>
  <section
    v-if="tags.length > 0"
    class="tags-group"
    aria-label="Tags"
  >
    <h2 class="group-title">
      Tags
    </h2>
    <ul>
      <li
        v-for="tag in tags"
        :key="tag.id"
      >
        <button
          type="button"
          class="tag"
          :class="{ selected: tag.id === activeTagId }"
          :aria-current="tag.id === activeTagId ? 'true' : undefined"
          @click="emit('selectTag', tag.id, tag.displayName)"
        >
          <span
            class="dot"
            :style="dotStyle(tag.color)"
            aria-hidden="true"
          />
          <span class="name">{{ tag.displayName }}</span>
        </button>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.tags-group {
  padding-top: var(--space-3);
  margin-top: var(--space-2);
  border-top: 1px solid var(--color-border);
}

.group-title {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 0 var(--space-2) var(--space-1);
}

.tag {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  transition: background var(--transition-fast);
}

.tag:hover {
  background: var(--color-bg-hover);
}

.tag.selected {
  background: var(--color-bg-selected);
  color: var(--color-text);
  font-weight: var(--weight-medium);
}

.dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex-shrink: 0;
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
