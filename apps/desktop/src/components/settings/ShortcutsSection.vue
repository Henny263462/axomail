<script setup lang="ts">
/**
 * The "Keyboard shortcuts" settings section: a read-only reference of the
 * currently bound shortcuts, grouped by area, generated from the command
 * registry. Rebinding is a planned follow-up; for now this mirrors the help
 * overlay inside Settings so the bindings are discoverable there too.
 *
 * A registry section: its only prop is the shared {@link SettingsSectionContext}
 * (unused here — this section reads the command registry, not the settings
 * form), keeping the section registry uniform.
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { shortcutGroups } from "../../commands/display";
import { useOptionalCommandRegistry } from "../../composables/useCommandRegistry";
import ShortcutList from "../command/ShortcutList.vue";
import type { SettingsSectionContext } from "./context";

defineProps<{ context: SettingsSectionContext }>();

const { t } = useI18n({ useScope: "global" });

// The settings dialog always mounts under the app root (which provides the
// registry); tolerate its absence so an isolated unit mount degrades to an
// empty list rather than throwing.
const registry = useOptionalCommandRegistry();
const groups = computed(() =>
  shortcutGroups(registry?.commands.value ?? []),
);
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      {{ t("settings.shortcuts.title") }}
    </legend>
    <p class="hint">
      {{ t("settings.shortcuts.hintBefore") }}
      <kbd class="inline">?</kbd> {{ t("settings.shortcuts.hintMiddle") }}
      <kbd class="inline">Ctrl</kbd>+<kbd class="inline">K</kbd> {{ t("settings.shortcuts.hintAfter") }}
    </p>
    <ShortcutList :groups="groups" />
  </fieldset>
</template>

<style scoped>
.group {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.legend {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 0 var(--space-1);
}

.hint {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  line-height: 1.5;
}

.inline {
  font-family: inherit;
  font-size: var(--text-xs);
  background: var(--color-bg-subtle);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  padding: 0 var(--space-1);
}
</style>
