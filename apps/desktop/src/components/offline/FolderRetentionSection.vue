<script setup lang="ts">
/**
 * The per-folder message-retention controls, mounted inside
 * {@link FolderPropertiesDialog}. Self-contained: it loads and saves the policy
 * through `api/retention` and can run the maintenance pass ("Apply now"),
 * reporting outcomes through an aria-live region.
 *
 * The maintenance pass is deliberately user-invoked (never silent): "Move to
 * Trash" reuses the guarded delete-to-Trash path and "Remove the local copy
 * only" drops just the cached body — neither ever hard-deletes server mail.
 */
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import {
  applyFolderRetention,
  getFolderRetention,
  setFolderRetention,
} from "../../api/retention";
import type { RetentionAction, RetentionMode } from "../../types/retention";

const props = defineProps<{ mailboxId: number }>();

const { t } = useI18n({ useScope: "global" });

type LoadState = "loading" | "ready" | "unavailable";
const state = ref<LoadState>("loading");
const mode = ref<RetentionMode>("keep_all");
const days = ref(30);
const maxCount = ref(1000);
const action = ref<RetentionAction>("trash");
const busy = ref(false);
const status = ref<string | null>(null);

async function load(): Promise<void> {
  const policy = await getFolderRetention(props.mailboxId);
  if (policy === null) {
    state.value = "unavailable";
    return;
  }
  mode.value = policy.mode;
  days.value = policy.days;
  maxCount.value = policy.maxCount;
  action.value = policy.action;
  state.value = "ready";
}

onMounted(load);

async function onSave(): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  status.value = null;
  try {
    await setFolderRetention(props.mailboxId, {
      mode: mode.value,
      days: Math.max(1, Math.floor(days.value) || 1),
      maxCount: Math.max(1, Math.floor(maxCount.value) || 1),
      action: action.value,
    });
    status.value = t("retention.saved");
  } catch {
    status.value = t("retention.saveFailed");
  } finally {
    busy.value = false;
  }
}

async function onApply(): Promise<void> {
  if (busy.value) return;
  // Persist the current choice first so the pass acts on what the user sees.
  await onSave();
  if (status.value === t("retention.saveFailed")) return;
  busy.value = true;
  try {
    const outcome = await applyFolderRetention(props.mailboxId);
    status.value =
      outcome === null
        ? t("retention.unavailable")
        : t("retention.applied", {
            trashed: outcome.movedToTrash,
            purged: outcome.purgedBodies,
          });
  } catch {
    status.value = t("retention.applyFailed");
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section
    class="retention"
    aria-labelledby="retention-title"
  >
    <h3
      id="retention-title"
      class="legend"
    >
      {{ t("retention.title") }}
    </h3>

    <p
      v-if="state === 'loading'"
      class="note"
      role="status"
    >
      {{ t("retention.title") }}…
    </p>
    <p
      v-else-if="state === 'unavailable'"
      class="note"
      role="status"
    >
      {{ t("retention.unavailable") }}
    </p>

    <div
      v-else
      class="body"
    >
      <p class="hint">
        {{ t("retention.hint") }}
      </p>

      <fieldset class="modes">
        <legend class="sr-only">
          {{ t("retention.title") }}
        </legend>
        <label class="choice">
          <input
            v-model="mode"
            type="radio"
            value="keep_all"
            name="retention-mode"
          >
          <span>{{ t("retention.modeKeepAll") }}</span>
        </label>
        <label class="choice">
          <input
            v-model="mode"
            type="radio"
            value="by_age"
            name="retention-mode"
          >
          <span>{{ t("retention.modeByAge") }}</span>
        </label>
        <label
          v-if="mode === 'by_age'"
          class="field"
        >
          <span>{{ t("retention.days") }}</span>
          <input
            v-model.number="days"
            type="number"
            min="1"
            :aria-label="t('retention.days')"
          >
        </label>
        <label class="choice">
          <input
            v-model="mode"
            type="radio"
            value="by_count"
            name="retention-mode"
          >
          <span>{{ t("retention.modeByCount") }}</span>
        </label>
        <label
          v-if="mode === 'by_count'"
          class="field"
        >
          <span>{{ t("retention.maxCount") }}</span>
          <input
            v-model.number="maxCount"
            type="number"
            min="1"
            :aria-label="t('retention.maxCount')"
          >
        </label>
      </fieldset>

      <label
        v-if="mode !== 'keep_all'"
        class="field action"
      >
        <span>{{ t("retention.action") }}</span>
        <select
          v-model="action"
          :aria-label="t('retention.action')"
        >
          <option value="trash">
            {{ t("retention.actionTrash") }}
          </option>
          <option value="purge_body">
            {{ t("retention.actionPurge") }}
          </option>
        </select>
      </label>
      <p
        v-if="mode !== 'keep_all'"
        class="hint"
      >
        {{ t("retention.actionHint") }}
      </p>

      <p
        v-if="status !== null"
        class="status"
        role="status"
        aria-live="polite"
      >
        {{ status }}
      </p>

      <div class="actions">
        <button
          type="button"
          class="button"
          :disabled="busy || mode === 'keep_all'"
          @click="onApply"
        >
          {{ busy ? t("retention.applying") : t("retention.apply") }}
        </button>
        <button
          type="button"
          class="button primary"
          :disabled="busy"
          @click="onSave"
        >
          {{ t("retention.save") }}
        </button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.retention {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding-top: var(--space-4);
  margin-top: var(--space-2);
  border-top: 1px solid var(--color-border);
}

.legend {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.note,
.hint {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.body {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.modes {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  border: none;
  padding: 0;
  margin: 0;
}

.choice {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
}

.field {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  margin-left: var(--space-5);
}

.field.action {
  margin-left: 0;
}

.field input,
.field select {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-text);
}

.field input {
  width: 6rem;
}

.status {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
}

.button {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-strong);
  color: var(--color-text);
  background: var(--color-bg);
  transition: background var(--transition-fast);
}

.button:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.button:disabled {
  opacity: 0.6;
  cursor: default;
}

.button.primary:not(:disabled) {
  color: var(--color-text-inverse);
  background: var(--color-accent);
  border-color: var(--color-accent);
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

@media (prefers-reduced-motion: reduce) {
  .button {
    transition: none;
  }
}
</style>
