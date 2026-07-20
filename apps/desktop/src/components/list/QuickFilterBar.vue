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
import { useI18n } from "vue-i18n";

import type { TagChip } from "../../types/mail";
import type { QuickFilter, SortField, SortSpec } from "./listFilter";

const { t } = useI18n({ useScope: "global" });

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
    :aria-label="t('list.quickFilter.filtersLabel')"
  >
    <div class="facets">
      <button
        type="button"
        class="chip"
        :class="{ on: filter.unread }"
        :aria-pressed="filter.unread"
        @click="patchFilter({ unread: !filter.unread })"
      >
        {{ t("list.quickFilter.unread") }}
      </button>
      <button
        type="button"
        class="chip"
        :class="{ on: filter.starred }"
        :aria-pressed="filter.starred"
        @click="patchFilter({ starred: !filter.starred })"
      >
        {{ t("list.quickFilter.starred") }}
      </button>
      <label
        v-if="availableTags.length > 0"
        class="picker"
      >
        <span class="sr-only">{{ t("list.quickFilter.filterByTag") }}</span>
        <select
          :value="filter.tagId === null ? '' : String(filter.tagId)"
          :aria-label="t('list.quickFilter.filterByTag')"
          @change="onTagChange"
        >
          <option value="">{{ t("list.quickFilter.allTags") }}</option>
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
      :placeholder="t('list.quickFilter.filterMessages')"
      :aria-label="t('list.quickFilter.filterMessagesByText')"
      @input="onText"
    >

    <div class="sort">
      <label class="picker">
        <span class="sr-only">{{ t("list.quickFilter.sortBy") }}</span>
        <select
          :value="sort.field"
          :aria-label="t('list.quickFilter.sortMessagesBy')"
          @change="onSortField"
        >
          <option value="date">{{ t("list.quickFilter.sortDate") }}</option>
          <option value="sender">{{ t("list.quickFilter.sortSender") }}</option>
          <option value="subject">{{ t("list.quickFilter.sortSubject") }}</option>
        </select>
      </label>
      <button
        type="button"
        class="dir"
        :aria-label="
          sort.dir === 'asc'
            ? t('list.quickFilter.sortAscending')
            : t('list.quickFilter.sortDescending')
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
      :aria-label="t('list.quickFilter.threadedLabel')"
      @click="emit('update:threaded', !threaded)"
    >
      {{ t("list.quickFilter.threaded") }}
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
