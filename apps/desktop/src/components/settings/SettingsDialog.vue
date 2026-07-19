<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";

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
import { settingsSections } from "./sections";

const props = defineProps<{
  /** Accounts, used to label each POP3 delete-policy toggle. */
  accounts: AccountSummary[];
}>();

const emit = defineEmits<{
  close: [];
  /** An account section edited or deleted an account; the shell must refresh. */
  accountsChanged: [];
}>();

type LoadState = "loading" | "ready" | "unavailable";
const state = ref<LoadState>("loading");
const saving = ref(false);
const errorMessage = ref("");
const statusMessage = ref("");
const root = ref<HTMLElement | null>(null);

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
  );
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
    errorMessage.value =
      "Saving settings needs the desktop app; it is unavailable in the browser preview.";
    return;
  }
  saving.value = true;
  try {
    await updateSettings(currentSettings());
    errorMessage.value = "";
    // The previewed theme is now persisted; make it the revert baseline so
    // close() does not undo it.
    savedTheme.value = form.theme;
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
      <h2
        id="settings-title"
        class="title"
      >
        Settings
      </h2>

      <p
        v-if="state === 'loading'"
        class="note"
        role="status"
      >
        Loading settings…
      </p>
      <p
        v-else-if="state === 'unavailable'"
        class="note"
        role="status"
      >
        Settings require the desktop app; they are unavailable in the browser
        preview.
      </p>

      <form
        v-else
        class="form"
        @submit.prevent="onSave"
      >
        <component
          :is="section.component"
          v-for="section in settingsSections"
          :key="section.id"
          :context="context"
        />

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
            Cancel
          </button>
          <button
            type="submit"
            class="button primary"
            :disabled="saving"
          >
            {{ saving ? "Saving…" : "Save" }}
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: var(--space-6) var(--space-4);
  overflow-y: auto;
  background: rgb(0 0 0 / 40%);
}

.dialog {
  width: 100%;
  max-width: 460px;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
}

.title {
  font-size: var(--text-lg);
  font-weight: var(--weight-semibold);
  letter-spacing: -0.01em;
  margin-bottom: var(--space-4);
}

.note {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.error {
  font-size: var(--text-sm);
  color: var(--color-danger);
}

.error:empty {
  display: none;
}

.status {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.status:empty {
  display: none;
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
</style>
