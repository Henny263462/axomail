<script setup lang="ts">
/**
 * The "find settings automatically" control for account setup: a trigger
 * button plus an honest status line (searching / found via a source / nothing
 * found / unavailable). It owns no lookup logic — the composable runs the
 * discovery and passes the status and source down.
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { AutoconfigSource } from "../../types/mail";
import type { AutoconfigStatus } from "./useAutoconfigLookup";

const props = defineProps<{
  status: AutoconfigStatus;
  source: AutoconfigSource | null;
}>();

const emit = defineEmits<{
  find: [];
}>();

const { t } = useI18n({ useScope: "global" });

/** Localized name of the source that produced a found configuration. */
const sourceLabel = computed(() => {
  switch (props.source) {
    case "autoconfig":
      return t("setup.autoconfig.sourceAutoconfig");
    case "ispdb":
      return t("setup.autoconfig.sourceIspdb");
    case "guess":
      return t("setup.autoconfig.sourceGuess");
    default:
      return "";
  }
});

/** The status message under the button, empty while idle. */
const message = computed(() => {
  switch (props.status) {
    case "searching":
      return t("setup.autoconfig.searching");
    case "found":
      return t("setup.autoconfig.foundVia", { source: sourceLabel.value });
    case "notFound":
      return t("setup.autoconfig.notFound");
    case "unavailable":
      return t("setup.autoconfig.unavailable");
    case "needEmail":
      return t("setup.autoconfig.needEmail");
    default:
      return "";
  }
});
</script>

<template>
  <div class="autoconfig">
    <button
      type="button"
      class="find"
      :disabled="status === 'searching'"
      @click="emit('find')"
    >
      {{ t("setup.autoconfig.action") }}
    </button>
    <p
      v-if="message !== ''"
      class="status"
      :class="{ found: status === 'found' }"
      role="status"
      aria-live="polite"
    >
      {{ message }}
    </p>
  </div>
</template>

<style scoped>
.autoconfig {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.find {
  align-self: flex-start;
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  color: var(--color-text);
  background: var(--color-bg);
  transition: background var(--transition-fast);
}

.find:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.find:disabled {
  opacity: 0.6;
  cursor: default;
}

.status {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.status.found {
  color: var(--color-accent);
}
</style>
