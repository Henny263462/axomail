<script setup lang="ts">
/**
 * The sidebar "Saved searches" group: a flat list of saved searches
 * (virtual folders). Clicking one runs its query and shows the results in the
 * list pane; each row also offers edit (re-open the builder) and delete.
 *
 * Deliberately a SEPARATE child component with its own data load, mounted at a
 * marked point in {@link SidebarPane}, so it never collides with the folder tree
 * or the Tags group. It owns the list and reloads whenever `refreshKey` changes
 * (a create/update/delete bumps it).
 */
import { onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import { deleteSavedSearch, listSavedSearches } from "../../api/savedSearch";
import type { SavedSearchDto } from "../../types/search";

const props = defineProps<{
  /** The saved search currently shown, for highlighting. */
  activeSavedSearchId?: number | null;
  /** Reload trigger: any change reloads the list. */
  refreshKey?: number;
}>();

const emit = defineEmits<{
  select: [saved: SavedSearchDto];
  edit: [saved: SavedSearchDto];
  changed: [];
}>();

const { t } = useI18n({ useScope: "global" });

const searches = ref<SavedSearchDto[]>([]);

async function load(): Promise<void> {
  searches.value = (await listSavedSearches()) ?? [];
}

onMounted(load);

watch(
  () => props.refreshKey,
  () => {
    void load();
  },
);

async function onDelete(saved: SavedSearchDto): Promise<void> {
  await deleteSavedSearch(saved.id);
  await load();
  emit("changed");
}
</script>

<template>
  <section
    v-if="searches.length > 0"
    class="saved-group"
    :aria-label="t('search.saved.group')"
  >
    <h2 class="group-title">
      {{ t("search.saved.group") }}
    </h2>
    <ul>
      <li
        v-for="saved in searches"
        :key="saved.id"
        class="item"
      >
        <button
          type="button"
          class="saved"
          :class="{ selected: saved.id === activeSavedSearchId }"
          :aria-current="saved.id === activeSavedSearchId ? 'true' : undefined"
          @click="emit('select', saved)"
        >
          <span
            class="glyph"
            aria-hidden="true"
          >⌕</span>
          <span class="name">{{ saved.name }}</span>
        </button>
        <button
          type="button"
          class="row-action"
          :aria-label="`${t('search.saved.edit')}: ${saved.name}`"
          :title="t('search.saved.edit')"
          @click="emit('edit', saved)"
        >
          ✎
        </button>
        <button
          type="button"
          class="row-action"
          :aria-label="`${t('search.saved.delete')}: ${saved.name}`"
          :title="t('search.saved.delete')"
          @click="onDelete(saved)"
        >
          ×
        </button>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.saved-group {
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

.item {
  display: flex;
  align-items: center;
  gap: 2px;
}

.saved {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex: 1;
  min-width: 0;
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  transition: background var(--transition-fast);
}

.saved:hover {
  background: var(--color-bg-hover);
}

.saved.selected {
  background: var(--color-bg-selected);
  color: var(--color-text);
  font-weight: var(--weight-medium);
}

.glyph {
  flex-shrink: 0;
  color: var(--color-text-tertiary);
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-action {
  flex-shrink: 0;
  width: 22px;
  height: 22px;
  border-radius: var(--radius-sm);
  color: var(--color-text-tertiary);
  font-size: var(--text-sm);
  line-height: 1;
  opacity: 0;
  transition: background var(--transition-fast), opacity var(--transition-fast);
}

.item:hover .row-action,
.row-action:focus-visible {
  opacity: 1;
}

.row-action:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}
</style>
