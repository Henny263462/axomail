<script setup lang="ts">
/**
 * Junk (spam) settings: enable the Bayesian classifier, tune the auto-file
 * threshold, trust server `X-Spam-*` headers, honor the address-book whitelist,
 * and run junk controls over every account's Inbox on demand. Autonomous — it
 * loads and persists through the junk api and reports outcomes via the dialog's
 * shared aria-live regions.
 */
import { onMounted, ref } from "vue";

import { getJunkSettings, runJunkControls, setJunkSettings } from "../../api/junk";
import { isTauri } from "../../lib/tauri";
import type { JunkSettings } from "../../types/mail";
import { messageFrom, type SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

/** Defaults mirror the backend so the browser preview shows sane values. */
const settings = ref<JunkSettings>({
  enabled: false,
  threshold: 0.9,
  trustServerHeaders: true,
  whitelistContacts: true,
  markReadOnJunk: false,
});
const running = ref(false);

/** The threshold shown as a whole-number percentage (0–100). */
const thresholdPercent = ref(90);
/** The last successfully-persisted threshold %, restored if a change fails. */
const committedPercent = ref(90);

onMounted(async () => {
  const loaded = await getJunkSettings();
  if (loaded !== null) {
    settings.value = loaded;
    thresholdPercent.value = Math.round(loaded.threshold * 100);
    committedPercent.value = thresholdPercent.value;
  }
});

/**
 * Persists the current settings. Returns whether it succeeded so callers revert
 * their optimistic change on failure or in the browser preview — an un-reverted
 * control would otherwise silently re-submit a change that never took, combining
 * it with the next edit.
 */
async function persist(): Promise<boolean> {
  props.context.reportStatus("");
  if (!isTauri()) {
    props.context.reportError(
      "Junk settings need the desktop app; they are unavailable in the browser preview.",
    );
    return false;
  }
  try {
    await setJunkSettings(settings.value);
    committedPercent.value = thresholdPercent.value;
    props.context.reportError("");
    return true;
  } catch (error) {
    props.context.reportError(messageFrom(error));
    return false;
  }
}

/** Applies a checkbox change optimistically, reverting on failure/preview. */
async function toggleBoolean(
  event: Event,
  key: "enabled" | "trustServerHeaders" | "whitelistContacts" | "markReadOnJunk",
): Promise<void> {
  const target = event.target as HTMLInputElement;
  const previous = settings.value[key];
  settings.value[key] = target.checked;
  if (!(await persist())) {
    settings.value[key] = previous;
    target.checked = previous;
  }
}

function onToggleEnabled(event: Event): void {
  void toggleBoolean(event, "enabled");
}

function onToggleTrust(event: Event): void {
  void toggleBoolean(event, "trustServerHeaders");
}

function onToggleWhitelist(event: Event): void {
  void toggleBoolean(event, "whitelistContacts");
}

function onToggleMarkRead(event: Event): void {
  void toggleBoolean(event, "markReadOnJunk");
}

function onThresholdInput(event: Event): void {
  const percent = Number((event.target as HTMLInputElement).value);
  const clamped = Math.min(Math.max(0, percent || 0), 100);
  thresholdPercent.value = clamped;
  settings.value.threshold = clamped / 100;
}

/** Commits a threshold drag; restores the last-good value if persistence fails. */
async function onThresholdCommit(event: Event): Promise<void> {
  if (!(await persist())) {
    thresholdPercent.value = committedPercent.value;
    settings.value.threshold = committedPercent.value / 100;
    (event.target as HTMLInputElement).value = String(committedPercent.value);
  }
}

/** Runs junk controls over every account's Inbox and reports the total filed. */
async function onRunControls(): Promise<void> {
  props.context.reportError("");
  if (!isTauri()) {
    props.context.reportError(
      "Running junk controls needs the desktop app; it is unavailable in the browser preview.",
    );
    return;
  }
  running.value = true;
  props.context.reportStatus("Running junk controls…");
  try {
    let filed = 0;
    for (const account of props.context.accounts) {
      for (const mailbox of account.mailboxes) {
        if (mailbox.role === "inbox") {
          filed += (await runJunkControls(mailbox.id)) ?? 0;
        }
      }
    }
    props.context.reportStatus(`Filed ${filed} message${filed === 1 ? "" : "s"} as junk.`);
  } catch (error) {
    props.context.reportError(messageFrom(error));
  } finally {
    running.value = false;
  }
}
</script>

<template>
  <div class="section">
    <label class="toggle">
      <input
        id="settings-junk-enabled"
        type="checkbox"
        :checked="settings.enabled"
        @change="onToggleEnabled"
      >
      <span>Enable junk filter (score incoming mail and file spam automatically)</span>
    </label>

    <div class="field">
      <label for="settings-junk-threshold">
        Junk threshold: {{ thresholdPercent }}%
      </label>
      <input
        id="settings-junk-threshold"
        type="range"
        min="0"
        max="100"
        step="5"
        :value="thresholdPercent"
        :disabled="!settings.enabled"
        @input="onThresholdInput"
        @change="onThresholdCommit"
      >
      <p class="hint">
        Mail scoring at or above this confidence is moved to Junk.
      </p>
    </div>

    <label class="toggle">
      <input
        id="settings-junk-trust"
        type="checkbox"
        :checked="settings.trustServerHeaders"
        @change="onToggleTrust"
      >
      <span>Trust server junk headers (honor X-Spam-Flag / X-Spam-Status)</span>
    </label>

    <label class="toggle">
      <input
        id="settings-junk-whitelist"
        type="checkbox"
        :checked="settings.whitelistContacts"
        @change="onToggleWhitelist"
      >
      <span>Never mark mail from my contacts as junk</span>
    </label>

    <label class="toggle">
      <input
        id="settings-junk-mark-read"
        type="checkbox"
        :checked="settings.markReadOnJunk"
        @change="onToggleMarkRead"
      >
      <span>Mark messages as read when they are filed as junk</span>
    </label>

    <div class="field">
      <button
        type="button"
        class="run"
        :disabled="running"
        @click="onRunControls"
      >
        {{ running ? "Running…" : "Run junk controls on Inbox" }}
      </button>
      <p class="hint">
        Re-scores every account's Inbox and files anything over the threshold.
      </p>
    </div>
  </div>
</template>

<style scoped>
.section {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.field label {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

.field input[type="range"] {
  max-width: 16rem;
}

.field input:disabled {
  opacity: 0.6;
}

.run {
  align-self: flex-start;
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  font-size: var(--text-sm);
  color: var(--color-text);
  cursor: pointer;
}

.run:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.run:disabled {
  opacity: 0.6;
  cursor: default;
}

.hint {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}
</style>
