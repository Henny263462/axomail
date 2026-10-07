<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ForwardStyle } from "../../types/compose";
import AppIcon from "../common/AppIcon.vue";
import DropdownMenu from "../common/DropdownMenu.vue";
import type { MenuItem } from "../common/menu";

withDefaults(defineProps<{ standalone?: boolean }>(), { standalone: false });

const { t } = useI18n({ useScope: "global" });

const emit = defineEmits<{
  (event: "reply"): void;
  (event: "reply-all"): void;
  (event: "forward", style?: ForwardStyle): void;
  (event: "archive"): void;
  (event: "move"): void;
  (event: "copy"): void;
  (event: "junk"): void;
  (event: "delete"): void;
  (event: "open-window"): void;
}>();

const forwardItems = computed<MenuItem[]>(() => [
  { id: "default", label: t("message.actionBar.forward"), icon: "forward" },
  { id: "inline", label: t("toolbar.forwardInline"), icon: "text" },
  { id: "attachment", label: t("toolbar.forwardAttachment"), icon: "copy" },
]);

function onForward(id: string): void {
  emit("forward", id === "inline" || id === "attachment" ? id : undefined);
}
</script>

<template>
  <div
    class="action-bar"
    role="toolbar"
    :aria-label="t('message.actionBar.ariaLabel')"
  >
    <button
      type="button"
      class="action primary"
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
    <div class="split">
      <button
        type="button"
        class="action"
        @click="emit('forward')"
      >
        <AppIcon name="forward" />
        {{ t("message.actionBar.forward") }}
      </button>
      <DropdownMenu
        class="split-more"
        :items="forwardItems"
        :label="t('toolbar.forwardMore')"
        @select="onForward"
      >
        <AppIcon
          name="chevronDown"
          :size="12"
        />
      </DropdownMenu>
    </div>
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
    <button
      v-if="!standalone"
      type="button"
      class="action icon-only"
      :aria-label="t('toolbar.openWindow')"
      :title="t('toolbar.openWindow')"
      @click="emit('open-window')"
    >
      <AppIcon name="externalWindow" />
    </button>
  </div>
</template>

<style scoped>
.action-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
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
  font-size: var(--text-sm);
  color: var(--color-text);
  cursor: pointer;
}

.action:hover {
  background: var(--color-bg-hover);
  border-color: var(--color-border-strong);
}

.action.primary {
  background: var(--color-accent);
  border-color: transparent;
  color: var(--color-text-inverse);
}

.action.primary:hover {
  background: var(--color-accent-hover);
}

.action.danger:hover {
  border-color: var(--color-danger);
  color: var(--color-danger);
}

.action.icon-only {
  margin-left: auto;
  padding: 5px 7px;
}

.split {
  display: inline-flex;
}

.split > .action {
  border-radius: var(--radius-md) 0 0 var(--radius-md);
}

.split-more :deep(.trigger) {
  height: 100%;
  padding: 0 6px;
  border: 1px solid var(--color-border);
  border-left: none;
  border-radius: 0 var(--radius-md) var(--radius-md) 0;
  background: var(--color-bg-elevated);
  color: var(--color-text-secondary);
}

.split-more :deep(.trigger:hover),
.split-more :deep(.trigger.open) {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.action-sep {
  width: 1px;
  align-self: stretch;
  margin: 0 var(--space-1);
  background: var(--color-border);
}
</style>
