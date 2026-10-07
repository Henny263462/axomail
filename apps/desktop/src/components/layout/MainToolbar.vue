<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useOptionalCommandRegistry } from "../../composables/useCommandRegistry";
import AppIcon from "../common/AppIcon.vue";
import DropdownMenu from "../common/DropdownMenu.vue";
import type { IconName } from "../common/icons";
import type { MenuItem } from "../common/menu";

interface ToolbarButton {
  command: string;
  icon: IconName;
  labelKey: string;
}

const { t } = useI18n({ useScope: "global" });
const registry = useOptionalCommandRegistry();

const commands = computed(() => registry?.commands.value ?? []);

function find(id: string) {
  return commands.value.find((command) => command.id === id);
}

function enabled(id: string): boolean {
  const command = find(id);
  if (command === undefined) return false;
  return command.isEnabled ? command.isEnabled() : true;
}

function run(id: string): void {
  const command = find(id);
  if (command && enabled(id)) void command.run();
}

function menu(entries: { command: string; icon: IconName; labelKey: string; separatorBefore?: boolean; danger?: boolean }[]): MenuItem[] {
  return entries.map((entry) => ({
    id: entry.command,
    icon: entry.icon,
    label: t(entry.labelKey),
    disabled: !enabled(entry.command),
    separatorBefore: entry.separatorBefore,
    danger: entry.danger,
  }));
}

const respond: ToolbarButton[] = [
  { command: "message.reply", icon: "reply", labelKey: "toolbar.reply" },
  { command: "message.replyAll", icon: "replyAll", labelKey: "toolbar.replyAll" },
];

const manage: ToolbarButton[] = [
  { command: "message.archive", icon: "archive", labelKey: "toolbar.archive" },
  { command: "message.move", icon: "folderMove", labelKey: "toolbar.move" },
  { command: "message.junk", icon: "junk", labelKey: "toolbar.junk" },
  { command: "message.delete", icon: "trash", labelKey: "toolbar.delete" },
];

const composeItems = computed(() =>
  menu([
    { command: "compose.new", icon: "compose", labelKey: "toolbar.newMessage" },
    { command: "compose.newFromTemplate", icon: "draft", labelKey: "toolbar.fromTemplate" },
  ]),
);

const forwardItems = computed(() =>
  menu([
    { command: "message.forward", icon: "forward", labelKey: "toolbar.forward" },
    { command: "message.forwardInline", icon: "text", labelKey: "toolbar.forwardInline" },
    { command: "message.forwardAttachment", icon: "copy", labelKey: "toolbar.forwardAttachment" },
  ]),
);

const markItems = computed(() =>
  menu([
    { command: "message.toggleUnread", icon: "mailOpen", labelKey: "toolbar.toggleRead" },
    { command: "message.toggleStar", icon: "star", labelKey: "toolbar.toggleStar" },
    { command: "message.copy", icon: "copy", labelKey: "toolbar.copy", separatorBefore: true },
    { command: "message.openWindow", icon: "externalWindow", labelKey: "toolbar.openWindow" },
    { command: "message.print", icon: "printer", labelKey: "toolbar.print" },
    { command: "message.notJunk", icon: "check", labelKey: "toolbar.notJunk", separatorBefore: true },
  ]),
);
</script>

<template>
  <div
    class="toolbar"
    role="toolbar"
    :aria-label="t('toolbar.label')"
  >
    <button
      type="button"
      class="tool"
      :disabled="!enabled('mail.getAll')"
      :title="t('toolbar.getMail')"
      @click="run('mail.getAll')"
    >
      <AppIcon
        name="refresh"
        :size="18"
      />
      <span class="label">{{ t("toolbar.getMail") }}</span>
    </button>

    <div class="split">
      <button
        type="button"
        class="tool"
        :disabled="!enabled('compose.new')"
        :title="t('toolbar.compose')"
        @click="run('compose.new')"
      >
        <AppIcon
          name="compose"
          :size="18"
        />
        <span class="label">{{ t("toolbar.compose") }}</span>
      </button>
      <DropdownMenu
        :items="composeItems"
        :label="t('toolbar.composeMore')"
        @select="run"
      >
        <AppIcon
          name="chevronDown"
          :size="12"
        />
      </DropdownMenu>
    </div>

    <span
      class="sep"
      aria-hidden="true"
    />

    <button
      v-for="button in respond"
      :key="button.command"
      type="button"
      class="tool"
      :disabled="!enabled(button.command)"
      :title="t(button.labelKey)"
      @click="run(button.command)"
    >
      <AppIcon
        :name="button.icon"
        :size="18"
      />
      <span class="label">{{ t(button.labelKey) }}</span>
    </button>

    <div class="split">
      <button
        type="button"
        class="tool"
        :disabled="!enabled('message.forward')"
        :title="t('toolbar.forward')"
        @click="run('message.forward')"
      >
        <AppIcon
          name="forward"
          :size="18"
        />
        <span class="label">{{ t("toolbar.forward") }}</span>
      </button>
      <DropdownMenu
        :items="forwardItems"
        :label="t('toolbar.forwardMore')"
        @select="run"
      >
        <AppIcon
          name="chevronDown"
          :size="12"
        />
      </DropdownMenu>
    </div>

    <span
      class="sep"
      aria-hidden="true"
    />

    <button
      v-for="button in manage"
      :key="button.command"
      type="button"
      class="tool"
      :class="{ danger: button.command === 'message.delete' }"
      :disabled="!enabled(button.command)"
      :title="t(button.labelKey)"
      @click="run(button.command)"
    >
      <AppIcon
        :name="button.icon"
        :size="18"
      />
      <span class="label">{{ t(button.labelKey) }}</span>
    </button>

    <DropdownMenu
      class="more"
      :items="markItems"
      :label="t('toolbar.more')"
      @select="run"
    >
      <span class="tool-face">
        <AppIcon
          name="more"
          :size="18"
        />
        <span class="label">{{ t("toolbar.more") }}</span>
      </span>
    </DropdownMenu>

    <span class="spacer" />

    <button
      type="button"
      class="tool"
      :disabled="!enabled('contacts.addressBook')"
      :title="t('toolbar.addressBook')"
      @click="run('contacts.addressBook')"
    >
      <AppIcon
        name="book"
        :size="18"
      />
      <span class="label">{{ t("toolbar.addressBook") }}</span>
    </button>
    <button
      type="button"
      class="tool"
      :disabled="!enabled('app.settings')"
      :title="t('toolbar.settings')"
      @click="run('app.settings')"
    >
      <AppIcon
        name="settings"
        :size="18"
      />
      <span class="label">{{ t("toolbar.settings") }}</span>
    </button>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 6px var(--space-2);
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg);
  flex-wrap: wrap;
  position: relative;
  z-index: 20;
}

.tool,
.tool-face {
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 3px;
  min-width: 64px;
  height: 52px;
  padding: 4px 8px;
  border-radius: var(--radius-md);
  color: var(--color-text);
  font-size: var(--text-xs);
  white-space: nowrap;
}

.tool:hover:not(:disabled),
.more :deep(.trigger:hover) .tool-face,
.more :deep(.trigger.open) .tool-face {
  background: var(--color-bg-hover);
}

.tool:disabled {
  color: var(--color-text-tertiary);
  cursor: default;
}

.tool.danger:hover:not(:disabled) {
  color: var(--color-danger);
}

.split {
  display: inline-flex;
  align-items: stretch;
  border-radius: var(--radius-md);
}

.split > .tool {
  border-radius: var(--radius-md) 0 0 var(--radius-md);
  padding-right: 4px;
}

.split :deep(.trigger) {
  height: 52px;
  padding: 0 4px;
  border-radius: 0 var(--radius-md) var(--radius-md) 0;
  color: var(--color-text-secondary);
}

.split :deep(.trigger:hover:not(:disabled)),
.split :deep(.trigger.open) {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.split:hover > .tool:not(:disabled) {
  background: var(--color-bg-subtle);
}

.sep {
  align-self: stretch;
  width: 1px;
  margin: 8px 6px;
  background: var(--color-border);
}

.spacer {
  flex: 1;
}

@media (max-width: 1080px) {
  .label {
    display: none;
  }

  .tool,
  .tool-face {
    min-width: 40px;
    height: 40px;
  }

  .split :deep(.trigger) {
    height: 40px;
  }
}
</style>
