<script setup lang="ts">
import { useI18n } from "vue-i18n";

import type { DraftDto } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";

defineProps<{
  drafts: DraftDto[];
  selectedDraftId: number | null;
}>();

const emit = defineEmits<{
  select: [draftId: number];
}>();

const { t } = useI18n({ useScope: "global" });

function summary(draft: DraftDto): string {
  const recipients = draft.to.join(", ");
  return recipients.length > 0 ? recipients : t("list.drafts.noRecipients");
}

function updated(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}
</script>

<template>
  <section
    class="list"
    :aria-label="t('list.drafts.label')"
  >
    <EmptyState
      v-if="drafts.length === 0"
      :title="t('list.drafts.emptyTitle')"
      :detail="t('list.drafts.emptyDetail')"
    />
    <ul v-else>
      <li
        v-for="draft in drafts"
        :key="draft.id"
      >
        <button
          type="button"
          class="item"
          :class="{ selected: draft.id === selectedDraftId }"
          :aria-current="draft.id === selectedDraftId ? 'true' : undefined"
          @click="emit('select', draft.id)"
        >
          <span class="row">
            <span class="subject">{{ draft.subject || t("list.noSubject") }}</span>
            <time class="date">{{ updated(draft.updatedUnix) }}</time>
          </span>
          <span class="to">{{ summary(draft) }}</span>
        </button>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.list {
  height: 100%;
  overflow-y: auto;
  border-right: 1px solid var(--color-border);
  background: var(--color-bg);
}

.item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 100%;
  text-align: left;
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
  transition: background var(--transition-fast);
}

.item:hover {
  background: var(--color-bg-hover);
}

.item.selected {
  background: var(--color-bg-selected);
}

.row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: var(--space-2);
}

.subject {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.date {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}

.to {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
