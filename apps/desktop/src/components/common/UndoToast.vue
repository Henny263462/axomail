<script setup lang="ts">
/**
 * Bottom-center toast area for message actions: a time-boxed **undo** affordance
 * after a move/archive/delete (with an Undo button), and a transient **notice**
 * (copy confirmation or an error). Both are polite live regions. Rendered at the
 * app root via a Teleport so they float above every pane.
 */
import { useI18n } from "vue-i18n";

defineProps<{
  /** The undo prompt (e.g. "Archived 3"), or null when there is nothing to undo. */
  undoMessage: string | null;
  /** A transient notice (e.g. "Copied to Archive"), or null. */
  notice: string | null;
}>();

const emit = defineEmits<{
  undo: [];
  dismiss: [];
}>();

const { t } = useI18n({ useScope: "global" });
</script>

<template>
  <Teleport to="body">
    <div
      class="toasts"
      aria-live="polite"
    >
      <div
        v-if="notice !== null"
        class="toast notice"
        role="status"
      >
        {{ notice }}
      </div>
      <div
        v-if="undoMessage !== null"
        class="toast undo"
        role="status"
      >
        <span class="label">{{ undoMessage }}</span>
        <button
          type="button"
          class="undo-button"
          @click="emit('undo')"
        >
          {{ t("common.undo") }}
        </button>
        <button
          type="button"
          class="close"
          :aria-label="t('common.dismiss')"
          @click="emit('dismiss')"
        >
          <span aria-hidden="true">×</span>
        </button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.toasts {
  position: fixed;
  left: 50%;
  bottom: var(--space-5);
  transform: translateX(-50%);
  z-index: 200;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  align-items: center;
  pointer-events: none;
}

.toast {
  pointer-events: auto;
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-strong);
  background: var(--color-bg-elevated, var(--color-bg));
  box-shadow: var(--shadow-md, 0 4px 12px rgb(0 0 0 / 20%));
  font-size: var(--text-sm);
  color: var(--color-text);
}

.label {
  font-weight: var(--weight-medium);
}

.undo-button {
  font: inherit;
  font-weight: var(--weight-semibold);
  color: var(--color-accent);
  background: none;
  border: none;
  cursor: pointer;
  padding: 0;
}

.undo-button:hover {
  text-decoration: underline;
}

.close {
  font-size: var(--text-md);
  line-height: 1;
  color: var(--color-text-tertiary);
  background: none;
  border: none;
  cursor: pointer;
  padding: 0 var(--space-1);
}

.close:hover {
  color: var(--color-text);
}
</style>
