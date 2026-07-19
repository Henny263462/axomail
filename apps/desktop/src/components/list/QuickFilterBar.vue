<script setup lang="ts">
/**
 * The quick-filter / sort / threading toolbar shown above the message list.
 *
 * Every control operates client-side over the already-loaded page(s): the
 * unread/starred facet chips, a tag picker (limited to tags present in the
 * loaded set), a free-text field, the sort field + direction, and the
 * threaded/flat toggle (the one persisted preference). Emits granular
 * `update:*` events; it never mutates the props it is given.
 */
import type { TagChip } from "../../types/mail";
import type { QuickFilter, SortField, SortSpec } from "./listFilter";

const props = defineProps<{
  filter: QuickFilter;
  sort: SortSpec;
  threaded: boolean;
  /** Tags present in the loaded set, offered in the tag picker. */
  availableTags: TagChip[];
}>();

const emit = defineEmits<{
  "update:filter": [value: QuickFilter];
  "update:sort": [value: SortSpec];
  "update:threaded": [value: boolean];
}>();

function patchFilter(patch: Partial<QuickFilter>): void {
  emit("update:filter", { ...props.filter, ...patch });
}

function onText(event: Event): void {
  patchFilter({ text: (event.target as HTMLInputElement).value });
}

function onTagChange(event: Event): void {
  const value = (event.target as HTMLSelectElement).value;
  patchFilter({ tagId: value === "" ? null : Number(value) });
}

function onSortField(event: Event): void {
  emit("update:sort", {
    ...props.sort,
    field: (event.target as HTMLSelectElement).value as SortField,
  });
}

function toggleDir(): void {
  emit("update:sort", {
    ...props.sort,
    dir: props.sort.dir === "asc" ? "desc" : "asc",
  });
}
</script>

<template>
  <div
    class="bar"
    role="toolbar"
    aria-label="Message list filters"
  >
    <div class="facets">
      <button
        type="button"
        class="chip"
        :class="{ on: filter.unread }"
        :aria-pressed="filter.unread"
        @click="patchFilter({ unread: !filter.unread })"
      >
        Unread
      </button>
      <button
        type="button"
        class="chip"
        :class="{ on: filter.starred }"
        :aria-pressed="filter.starred"
        @click="patchFilter({ starred: !filter.starred })"
      >
        Starred
      </button>
      <label
        v-if="availableTags.length > 0"
        class="picker"
      >
        <span class="sr-only">Filter by tag</span>
        <select
          :value="filter.tagId === null ? '' : String(filter.tagId)"
          aria-label="Filter by tag"
          @change="onTagChange"
        >
          <option value="">All tags</option>
          <option
            v-for="tag in availableTags"
            :key="tag.id"
            :value="String(tag.id)"
          >
            {{ tag.displayName }}
          </option>
        </select>
      </label>
    </div>

    <input
      type="search"
      class="text"
      :value="filter.text"
      placeholder="Filter messages"
      aria-label="Filter messages by text"
      @input="onText"
    >

    <div class="sort">
      <label class="picker">
        <span class="sr-only">Sort by</span>
        <select
          :value="sort.field"
          aria-label="Sort messages by"
          @change="onSortField"
        >
          <option value="date">Date</option>
          <option value="sender">Sender</option>
          <option value="subject">Subject</option>
        </select>
      </label>
      <button
        type="button"
        class="dir"
        :aria-label="
          sort.dir === 'asc' ? 'Sort ascending' : 'Sort descending'
        "
        @click="toggleDir"
      >
        <span aria-hidden="true">{{ sort.dir === "asc" ? "↑" : "↓" }}</span>
      </button>
    </div>

    <button
      type="button"
      class="chip thread-toggle"
      :class="{ on: threaded }"
      :aria-pressed="threaded"
      aria-label="Group messages into conversation threads"
      @click="emit('update:threaded', !threaded)"
    >
      Threaded
    </button>
  </div>
</template>

<style scoped>
.bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1) var(--space-2);
  padding: var(--space-1) var(--space-2);
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg-subtle);
}

.facets,
.sort {
  display: flex;
  align-items: center;
  gap: var(--space-1);
}

.chip {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
  padding: 2px var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  transition:
    background var(--transition-fast),
    color var(--transition-fast);
}

.chip:hover {
  background: var(--color-bg-hover);
}

.chip.on {
  color: var(--color-text-inverse);
  background: var(--color-accent);
  border-color: var(--color-accent);
}

.chip:focus-visible,
.dir:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: 1px;
}

.text {
  flex: 1;
  min-width: 6rem;
  font-size: var(--text-sm);
  padding: 2px var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-text);
}

.picker select {
  font-size: var(--text-xs);
  padding: 2px var(--space-1);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-text);
}

.dir {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  padding: 0 var(--space-1);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
</style>
