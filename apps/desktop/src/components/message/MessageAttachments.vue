<script setup lang="ts">
/**
 * The reading pane's attachment strip: one chip per downloadable attachment
 * with a Save action, plus Save all when there is more than one. The user
 * picks a destination through the native dialog (ADR-0010) and the bytes are
 * written by a Rust command; nothing is read or written by the webview. Silent
 * in the browser preview, where the dialog/bridge is absent.
 */
import { ref } from "vue";

import {
  pickDirectoryPath,
  pickSaveFilePath,
  saveAllAttachments,
  saveAttachment,
} from "../../api/attachments";
import type { AttachmentMetaDto } from "../../types/mail";
import { formatBytes } from "./format";

const props = defineProps<{
  messageId: number;
  attachments: AttachmentMetaDto[];
}>();

const busy = ref(false);
const status = ref<string | null>(null);

async function onSave(index: number, filename: string): Promise<void> {
  if (busy.value) return;
  const dest = await pickSaveFilePath(filename || "attachment");
  if (dest === null) return; // cancelled or no desktop bridge
  busy.value = true;
  status.value = null;
  try {
    const saved = await saveAttachment(props.messageId, index, dest);
    status.value = saved === null ? null : `Saved ${saved.filename}.`;
  } catch {
    status.value = "Could not save the attachment.";
  } finally {
    busy.value = false;
  }
}

async function onSaveAll(): Promise<void> {
  if (busy.value) return;
  const dir = await pickDirectoryPath();
  if (dir === null) return;
  busy.value = true;
  status.value = null;
  try {
    const saved = await saveAllAttachments(props.messageId, dir);
    status.value =
      saved === null
        ? null
        : `Saved ${saved.length} ${saved.length === 1 ? "attachment" : "attachments"}.`;
  } catch {
    status.value = "Could not save the attachments.";
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="attachments-area">
    <ul
      class="attachments"
      aria-label="Attachments"
    >
      <li
        v-for="(attachment, index) in attachments"
        :key="index"
        class="chip"
      >
        <span class="chip-name">{{ attachment.filename || "(unnamed)" }}</span>
        <span class="chip-size">{{ formatBytes(attachment.size) }}</span>
        <button
          type="button"
          class="chip-action"
          :disabled="busy"
          :aria-label="`Save ${attachment.filename || 'attachment'}`"
          @click="onSave(index, attachment.filename)"
        >
          Save
        </button>
      </li>
    </ul>
    <div
      v-if="attachments.length > 1"
      class="bulk"
    >
      <button
        type="button"
        class="bulk-action"
        :disabled="busy"
        @click="onSaveAll"
      >
        Save all
      </button>
    </div>
    <p
      v-if="status !== null"
      class="status"
      role="status"
      aria-live="polite"
    >
      {{ status }}
    </p>
  </div>
</template>

<style scoped>
.attachments-area {
  margin-top: var(--space-3);
}

.attachments {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.chip {
  display: inline-flex;
  align-items: baseline;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.chip-size {
  color: var(--color-text-tertiary);
}

.chip-action {
  align-self: center;
  padding: 2px var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  font-size: var(--text-xs);
  color: var(--color-accent);
  cursor: pointer;
}

.chip-action:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.chip-action:disabled {
  color: var(--color-text-tertiary);
  cursor: default;
}

.bulk {
  margin-top: var(--space-2);
}

.bulk-action {
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  font-size: var(--text-xs);
  color: var(--color-accent);
  cursor: pointer;
}

.bulk-action:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.bulk-action:disabled {
  color: var(--color-text-tertiary);
  cursor: default;
}

.status {
  margin-top: var(--space-2);
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}
</style>
