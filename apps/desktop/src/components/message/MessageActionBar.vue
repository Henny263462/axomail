<script setup lang="ts">
/**
 * The reading-pane action toolbar: Reply / Reply all / Forward, then the
 * relocation + Junk + Delete actions. Split out of {@link MessageDetail} to keep
 * that file under the size gate. Pure presentation — each control emits and the
 * host (MessageDetail → MessageViewPane) performs the action.
 */
import { useI18n } from "vue-i18n";

import AppIcon from "../common/AppIcon.vue";

const { t } = useI18n({ useScope: "global" });

const emit = defineEmits<{
  (event: "reply"): void;
  (event: "reply-all"): void;
  (event: "forward"): void;
  (event: "archive"): void;
  (event: "move"): void;
  (event: "copy"): void;
  (event: "junk"): void;
  (event: "delete"): void;
}>();
</script>

<template>
  <div
    class="action-bar"
    role="toolbar"
    :aria-label="t('message.actionBar.ariaLabel')"
  >
    <button
      type="button"
      class="action"
      @click="emit('reply')"
    >
      <AppIcon name="reply" />
      {{ t("message.actionBar.reply") }}
    </button>
    <button
      type="button"
      class="action"
      @click="emit('reply-all')"
    >
      <AppIcon name="replyAll" />
      {{ t("message.actionBar.replyAll") }}
    </button>
    <button
      type="button"
      class="action"
      @click="emit('forward')"
    >
      <AppIcon name="forward" />
      {{ t("message.actionBar.forward") }}
    </button>
    <span
      class="action-sep"
      aria-hidden="true"
    />
    <button
      type="button"
      class="action"
      @click="emit('archive')"
    >
      <AppIcon name="archive" />
      {{ t("message.actionBar.archive") }}
    </button>
    <button
      type="button"
      class="action"
      @click="emit('move')"
    >
      <AppIcon name="folderMove" />
      {{ t("message.actionBar.moveTo") }}
    </button>
    <button
      type="button"
      class="action"
      @click="emit('copy')"
    >
      <AppIcon name="copy" />
      {{ t("message.actionBar.copyTo") }}
    </button>
    <button
      type="button"
      class="action"
      @click="emit('junk')"
    >
      <AppIcon name="junk" />
      {{ t("message.actionBar.junk") }}
    </button>
    <button
      type="button"
      class="action danger"
      @click="emit('delete')"
    >
      <AppIcon name="trash" />
      {{ t("common.delete") }}
    </button>
  </div>
</template>

<style scoped>
.action-bar {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  margin-bottom: var(--space-4);
}

.action {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px var(--space-3);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-bg-elevated);
  box-shadow: var(--shadow-sm);
  font-size: var(--text-sm);
  color: var(--color-text);
  cursor: pointer;
}

.action:hover {
  background: var(--color-bg-hover);
  border-color: var(--color-border-strong);
}

.action:first-child {
  background: var(--color-accent);
  border-color: transparent;
  color: var(--color-text-inverse);
}

.action:first-child:hover {
  background: var(--color-accent-hover);
}

.action:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: 2px;
}

.action.danger {
  color: var(--color-danger);
}

.action.danger:hover {
  background: var(--color-danger-subtle);
  border-color: var(--color-danger);
}

.action-sep {
  width: 1px;
  align-self: stretch;
  margin: 0 var(--space-1);
  background: var(--color-border);
}
</style>
