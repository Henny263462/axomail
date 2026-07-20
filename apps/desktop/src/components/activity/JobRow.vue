<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { SyncJobDto, SyncJobKind, SyncJobState } from "../../types/mail";

const props = defineProps<{
  /** The job to render. */
  job: SyncJobDto;
  /** Whether a cancel is in flight (disables the button). */
  busy: boolean;
  /** Whether this job can still be cancelled (running or queued). */
  cancellable: boolean;
}>();

const emit = defineEmits<{
  cancel: [id: number];
}>();

const { t } = useI18n({ useScope: "global" });

/** Human-readable label for each job kind. */
const KIND_LABELS = computed<Record<SyncJobKind, string>>(() => ({
  discover: t("activity.kind.discover"),
  mailboxSync: t("activity.kind.mailboxSync"),
  fetch: t("activity.kind.fetch"),
  send: t("activity.kind.send"),
  opReplay: t("activity.kind.opReplay"),
}));

/** Human-readable label for each lifecycle state. */
const STATE_LABELS = computed<Record<SyncJobState, string>>(() => ({
  pending: t("activity.state.pending"),
  running: t("activity.state.running"),
  succeeded: t("activity.state.succeeded"),
  failed: t("activity.state.failed"),
  cancelled: t("activity.state.cancelled"),
}));

const title = computed(
  () =>
    props.job.mailboxLabel ??
    props.job.accountLabel ??
    t("activity.jobFallback", { id: props.job.id }),
);

/** Kind, optionally suffixed with the account when a mailbox is also named. */
const context = computed(() => {
  const parts = [KIND_LABELS.value[props.job.kind]];
  if (props.job.mailboxLabel !== null && props.job.accountLabel !== null) {
    parts.push(props.job.accountLabel);
  }
  return parts.join(" · ");
});
</script>

<template>
  <li class="job">
    <span
      class="badge"
      :class="job.state"
    >{{ STATE_LABELS[job.state] }}</span>
    <span class="info">
      <span class="job-title">{{ title }}</span>
      <span class="context">{{ context }}</span>
      <span
        v-if="job.error"
        class="error"
      >{{ job.error }}</span>
    </span>
    <button
      v-if="cancellable"
      type="button"
      class="button small"
      :disabled="busy"
      :aria-label="t('activity.cancelJobAria', { title })"
      @click="emit('cancel', job.id)"
    >
      {{ t("common.cancel") }}
    </button>
  </li>
</template>

<style scoped>
.job {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  padding: var(--space-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
}

.info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-width: 0;
}

.job-title {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.context {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.error {
  font-size: var(--text-xs);
  color: var(--color-danger);
}

.badge {
  flex-shrink: 0;
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  padding: 2px var(--space-2);
  border-radius: var(--radius-sm);
  background: var(--color-bg-subtle);
  color: var(--color-text-secondary);
}

.badge.running {
  background: var(--color-accent-subtle);
  color: var(--color-accent);
}

.badge.failed {
  color: var(--color-danger);
}

.button {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-strong);
  color: var(--color-text);
  background: var(--color-bg);
  transition: background var(--transition-fast);
}

.button:hover {
  background: var(--color-bg-hover);
}

.button:disabled {
  opacity: 0.6;
  cursor: default;
}

.button.small {
  flex-shrink: 0;
  padding: var(--space-1) var(--space-3);
}
</style>
