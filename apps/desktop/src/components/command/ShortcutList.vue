<script setup lang="ts">
/**
 * Presentational listing of keyboard shortcuts grouped by area. Shared by the
 * shortcut-help overlay and the Shortcuts settings section so both render the
 * bindings identically. Purely display: it takes the grouped commands and shows
 * each command's title next to its formatted shortcut.
 */
import { useI18n } from "vue-i18n";

import { formatShortcutHint } from "../../commands/display";
import type { CommandGroup } from "../../commands/display";

defineProps<{
  /** The shortcut groups to render, in display order. */
  groups: CommandGroup[];
}>();

const { t } = useI18n({ useScope: "global" });
</script>

<template>
  <div class="groups">
    <p
      v-if="groups.length === 0"
      class="empty"
      role="status"
    >
      {{ t("command.list.empty") }}
    </p>
    <section
      v-for="group in groups"
      :key="group.id"
      class="group"
    >
      <h3 class="heading">
        {{ group.label }}
      </h3>
      <dl class="rows">
        <div
          v-for="command in group.commands"
          :key="command.id"
          class="row"
        >
          <dt class="label">
            {{ command.title }}
          </dt>
          <dd class="binding">
            <kbd class="shortcut">{{ formatShortcutHint(command.shortcut) }}</kbd>
          </dd>
        </div>
      </dl>
    </section>
  </div>
</template>

<style scoped>
.groups {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.group {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.heading {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.rows {
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.label {
  font-size: var(--text-sm);
  color: var(--color-text);
}

.binding {
  margin: 0;
}

.shortcut {
  font-size: var(--text-xs);
  font-family: inherit;
  color: var(--color-text-secondary);
  background: var(--color-bg-subtle);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  padding: 0 var(--space-2);
  white-space: nowrap;
}

.empty {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}
</style>
