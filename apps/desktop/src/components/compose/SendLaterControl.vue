<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";

import {
  datetimeLocalToUnix,
  SEND_LATER_PRESETS,
  type SendLaterPreset,
} from "../../lib/sendLater";

defineProps<{
  /** True while a send is in flight; disables the whole control. */
  busy: boolean;
  /** True while another mutually-exclusive action (e.g. saving) runs. */
  disabled: boolean;
}>();

const emit = defineEmits<{
  /** Schedule the message for the given Unix second. */
  schedule: [unixSeconds: number];
}>();

const { t } = useI18n({ useScope: "global" });

const menuOpen = ref(false);
const customValue = ref("");
const customError = ref("");

function toggleMenu(): void {
  menuOpen.value = !menuOpen.value;
  customError.value = "";
}

/** Closes the menu when focus leaves the control entirely. */
function onFocusOut(event: FocusEvent): void {
  const next = event.relatedTarget as Node | null;
  if (next === null || !(event.currentTarget as HTMLElement).contains(next)) {
    menuOpen.value = false;
  }
}

function choosePreset(preset: SendLaterPreset): void {
  emit("schedule", preset.at(Date.now()));
  menuOpen.value = false;
}

function scheduleCustom(): void {
  const unix = datetimeLocalToUnix(customValue.value);
  if (unix === null) {
    customError.value = t("compose.sendLater.errorNoDateTime");
    return;
  }
  if (unix <= Math.floor(Date.now() / 1000)) {
    customError.value = t("compose.sendLater.errorPastTime");
    return;
  }
  emit("schedule", unix);
  menuOpen.value = false;
}
</script>

<template>
  <div
    class="send-later"
    @focusout="onFocusOut"
  >
    <button
      type="submit"
      class="button primary send"
      :disabled="busy || disabled"
    >
      {{ busy ? t("compose.sendLater.sending") : t("common.send") }}
    </button>
    <button
      type="button"
      class="button primary caret"
      :disabled="busy || disabled"
      :aria-label="t('compose.sendLater.sendLater')"
      :aria-expanded="menuOpen"
      aria-haspopup="menu"
      @click="toggleMenu"
    >
      ▾
    </button>

    <div
      v-if="menuOpen"
      class="menu"
      role="menu"
      :aria-label="t('compose.sendLater.options')"
    >
      <button
        v-for="preset in SEND_LATER_PRESETS"
        :key="preset.id"
        type="button"
        class="menu-item"
        role="menuitem"
        @click="choosePreset(preset)"
      >
        {{ t(preset.labelKey) }}
      </button>

      <div class="custom">
        <label for="send-later-custom">{{ t("compose.sendLater.atSpecificTime") }}</label>
        <input
          id="send-later-custom"
          v-model="customValue"
          type="datetime-local"
          @keydown.enter.prevent="scheduleCustom"
        >
        <button
          type="button"
          class="button schedule"
          @click="scheduleCustom"
        >
          {{ t("compose.sendLater.schedule") }}
        </button>
        <p
          class="error"
          role="alert"
          aria-live="assertive"
        >
          {{ customError }}
        </p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.send-later {
  position: relative;
  display: inline-flex;
}

.send {
  border-top-right-radius: 0;
  border-bottom-right-radius: 0;
}

.caret {
  border-top-left-radius: 0;
  border-bottom-left-radius: 0;
  border-left: 1px solid var(--color-text-inverse);
  padding-inline: var(--space-2);
}

.menu {
  position: absolute;
  right: 0;
  bottom: calc(100% + var(--space-1));
  z-index: 10;
  display: flex;
  flex-direction: column;
  min-width: 14rem;
  padding: var(--space-1);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  box-shadow: 0 8px 24px rgb(0 0 0 / 18%);
}

.menu-item {
  text-align: left;
  font-size: var(--text-sm);
  color: var(--color-text);
  padding: var(--space-2);
  border-radius: var(--radius-sm);
}

.menu-item:hover {
  background: var(--color-bg-hover);
}

.custom {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  padding: var(--space-2);
  border-top: 1px solid var(--color-border);
  margin-top: var(--space-1);
}

.custom label {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

.custom input {
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
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

.button:hover {
  background: var(--color-bg-hover);
}

.button:disabled {
  opacity: 0.6;
  cursor: default;
}

.button.primary {
  color: var(--color-text-inverse);
  background: var(--color-accent);
  border-color: var(--color-accent);
}

.button.primary:hover {
  background: var(--color-accent-hover);
}

.schedule {
  align-self: flex-start;
}

.error {
  font-size: var(--text-xs);
  color: var(--color-danger);
}

.error:empty {
  display: none;
}
</style>
