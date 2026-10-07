<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";

import { getSettings, updateSettings } from "../../api/settings";
import { useTheme } from "../../composables/useTheme";
import { isTauri } from "../../lib/tauri";
import type {
  AccountSummary,
  Pop3DeletePolicyDto,
  SettingsDto,
  ThemePreference,
} from "../../types/mail";
import { messageFrom, type SettingsSectionContext } from "./context";
import AppIcon from "../common/AppIcon.vue";
import { settingsGroups, settingsSections, type SettingsGroupId } from "./sections";

const props = defineProps<{
  /** Accounts, used to label each POP3 delete-policy toggle. */
  accounts: AccountSummary[];
}>();

const emit = defineEmits<{
  close: [];
  /** An account section edited or deleted an account; the shell must refresh. */
  accountsChanged: [];
  /** The tag set changed; the shell must refresh the sidebar Tags group. */
  tagsChanged: [];
  /** Settings were saved; the shell re-reads them (e.g. auto-read preference). */
  settingsSaved: [];
}>();

const { t } = useI18n({ useScope: "global" });

type LoadState = "loading" | "ready" | "unavailable";
const state = ref<LoadState>("loading");
const saving = ref(false);
const errorMessage = ref("");
const statusMessage = ref("");
const root = ref<HTMLElement | null>(null);
const activeGroup = ref<SettingsGroupId>("general");

// Live theme control: `applyTheme` previews a selection immediately; the
// preference is still batch-saved with the rest of the settings, and an
// unsaved preview is reverted to `savedTheme` when the dialog closes.
const { theme: activeTheme, setTheme: applyTheme } = useTheme();

// The three batch-saved general fields plus the loaded POP3 snapshot. The
// backend ignores the echoed policies (see update_settings), so keeping the
// loaded copy here is enough to satisfy the DTO shape on save.
const form = reactive<{
  notificationsEnabled: boolean;
  syncIntervalMinutes: number;
  theme: ThemePreference;
  autoMarkReadEnabled: boolean;
  autoMarkReadDelayMs: number;
}>({
  notificationsEnabled: false,
  syncIntervalMinutes: 5,
  theme: activeTheme.value,
  autoMarkReadEnabled: true,
  autoMarkReadDelayMs: 0,
});
const loadedPolicies = ref<Pop3DeletePolicyDto[]>([]);
// The last persisted theme; the revert baseline for an unsaved live preview.
const savedTheme = ref<ThemePreference>(activeTheme.value);

// A stable, read-through context (getters keep reactivity without exposing a
// mutable prop). Sections read the fields and push edits via the callbacks.
const context: SettingsSectionContext = {
  get accounts() {
    return props.accounts;
  },
  get initialPolicies() {
    return loadedPolicies.value;
  },
  get notificationsEnabled() {
    return form.notificationsEnabled;
  },
  get syncIntervalMinutes() {
    return form.syncIntervalMinutes;
  },
  get theme() {
    return form.theme;
  },
  get autoMarkReadEnabled() {
    return form.autoMarkReadEnabled;
  },
  get autoMarkReadDelayMs() {
    return form.autoMarkReadDelayMs;
  },
  setNotificationsEnabled(value) {
    form.notificationsEnabled = value;
  },
  setSyncIntervalMinutes(value) {
    form.syncIntervalMinutes = value;
  },
  setAutoMarkRead(enabled, delayMs) {
    form.autoMarkReadEnabled = enabled;
    form.autoMarkReadDelayMs = Math.max(0, delayMs);
  },
  setTheme(value) {
    form.theme = value;
    applyTheme(value);
  },
  reportError(message) {
    errorMessage.value = message;
  },
  reportStatus(message) {
    statusMessage.value = message;
  },
  notifyAccountsChanged() {
    emit("accountsChanged");
  },
  notifyTagsChanged() {
    emit("tagsChanged");
  },
};

onMounted(async () => {
  const settings = await getSettings();
  if (settings === null) {
    state.value = "unavailable";
    return;
  }
  form.notificationsEnabled = settings.notificationsEnabled;
  form.syncIntervalMinutes = Math.max(1, settings.syncIntervalMinutes);
  form.theme = settings.theme;
  form.autoMarkReadEnabled = settings.autoMarkReadEnabled;
  form.autoMarkReadDelayMs = Math.max(0, settings.autoMarkReadDelayMs);
  savedTheme.value = settings.theme;
  loadedPolicies.value = settings.pop3DeletePolicies.map((policy) => ({
    ...policy,
  }));
  state.value = "ready";
});

function close(): void {
  // Undo an unsaved live theme preview so cancelling leaves the theme as it
  // was persisted. After a successful save the baseline already matches, so
  // this is a no-op on that path.
  if (form.theme !== savedTheme.value) applyTheme(savedTheme.value);
  emit("close");
}

function focusables(): HTMLElement[] {
  if (root.value === null) return [];
  return Array.from(
    root.value.querySelectorAll<HTMLElement>(
      "button:not([disabled]), input:not([disabled])",
    ),
  ).filter((element) => element.closest("[data-inactive]") === null);
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    close();
    return;
  }
  if (event.key !== "Tab") return;
  const items = focusables();
  if (items.length === 0) return;
  const first = items[0];
  const last = items[items.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
}

function currentSettings(): SettingsDto {
  return {
    notificationsEnabled: form.notificationsEnabled,
    syncIntervalMinutes: Math.max(1, Math.floor(form.syncIntervalMinutes || 1)),
    theme: form.theme,
    autoMarkReadEnabled: form.autoMarkReadEnabled,
    autoMarkReadDelayMs: Math.max(0, Math.floor(form.autoMarkReadDelayMs || 0)),
    pop3DeletePolicies: loadedPolicies.value.map((policy) => ({ ...policy })),
  };
}

async function onSave(): Promise<void> {
  if (saving.value || state.value !== "ready") return;
  statusMessage.value = "";
  if (!isTauri()) {
    errorMessage.value = t("settings.dialog.saveUnavailable");
    return;
  }
  saving.value = true;
  try {
    await updateSettings(currentSettings());
    errorMessage.value = "";
    // The previewed theme is now persisted; make it the revert baseline so
    // close() does not undo it.
    savedTheme.value = form.theme;
    // Let the shell re-read the persisted settings so a mid-session change
    // (e.g. the auto-mark-read preference) takes effect immediately.
    emit("settingsSaved");
    close();
  } catch (error) {
    errorMessage.value = messageFrom(error);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div
    class="overlay"
    @keydown="onKeydown"
  >
    <div
      ref="root"
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-title"
    >
      <aside class="nav">
        <h2
          id="settings-title"
          class="title"
        >
          {{ t("settings.dialog.title") }}
        </h2>
        <div
          class="tabs"
          role="tablist"
          aria-orientation="vertical"
          :aria-label="t('settings.dialog.title')"
        >
          <button
            v-for="group in settingsGroups"
            :key="group.id"
            type="button"
            role="tab"
            class="tab"
            :class="{ active: activeGroup === group.id }"
            :aria-selected="activeGroup === group.id"
            @click="activeGroup = group.id"
          >
            <AppIcon :name="group.icon" />
            {{ group.title }}
          </button>
        </div>
      </aside>

      <section class="main">
        <p
          v-if="state === 'loading'"
          class="note"
          role="status"
        >
          {{ t("settings.dialog.loading") }}
        </p>
        <p
          v-else-if="state === 'unavailable'"
          class="note"
          role="status"
        >
          {{ t("settings.dialog.unavailable") }}
        </p>

        <form
          v-else
          class="form"
          @submit.prevent="onSave"
        >
          <div
            :key="activeGroup"
            class="content"
            role="tabpanel"
          >
            <div
              v-for="section in settingsSections"
              v-show="section.group === activeGroup"
              :key="section.id"
              :data-inactive="section.group === activeGroup ? undefined : 'true'"
            >
              <component
                :is="section.component"
                :context="context"
              />
            </div>
          </div>

          <footer class="footer">
            <p
              class="error"
              role="alert"
              aria-live="assertive"
            >
              {{ errorMessage }}
            </p>
            <p
              class="status"
              role="status"
              aria-live="polite"
            >
              {{ statusMessage }}
            </p>
            <div class="actions">
              <button
                type="button"
                class="button"
                @click="close"
              >
                {{ t("common.cancel") }}
              </button>
              <button
                type="submit"
                class="button primary"
                :disabled="saving"
              >
                {{ saving ? t("common.saving") : t("common.save") }}
              </button>
            </div>
          </footer>
        </form>
      </section>
    </div>
  </div>
</template>

<style scoped src="./SettingsDialog.css"></style>
