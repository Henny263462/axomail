<script setup lang="ts">
import { ref, watch } from "vue";

import { getMessageDetail, setRemoteContentAllowed } from "../../api/messages";
import type { MessageDetailDto } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";
import MessageDetail from "./MessageDetail.vue";

const props = defineProps<{
  messageId: number | null;
}>();

type LoadState = "idle" | "loading" | "loaded" | "error";

const state = ref<LoadState>("idle");
const detail = ref<MessageDetailDto | null>(null);

/**
 * Loads the detail for `id`, ignoring the result if the selection changed
 * while the request was in flight (last-write-wins by comparing ids).
 *
 * `allowRemote` is the allow-once override for this load; `null` defers to
 * the stored per-sender decision.
 */
async function load(id: number, allowRemote: boolean | null): Promise<void> {
  state.value = "loading";
  detail.value = null;
  const result = await getMessageDetail(id, allowRemote);
  if (props.messageId !== id) return;
  if (result === null) {
    state.value = "error";
    return;
  }
  detail.value = result;
  state.value = "loaded";
}

/** Re-fetches the current message showing remote content just this once. */
function showRemoteOnce(): void {
  if (props.messageId !== null) void load(props.messageId, true);
}

/**
 * Persists an "always allow" decision for `sender`, then re-fetches so the
 * stored decision (now allowing) takes effect.
 */
async function alwaysAllowRemote(sender: string): Promise<void> {
  const id = props.messageId;
  if (id === null) return;
  await setRemoteContentAllowed(sender, true);
  if (props.messageId === id) void load(id, null);
}

watch(
  () => props.messageId,
  (id) => {
    if (id === null) {
      state.value = "idle";
      detail.value = null;
      return;
    }
    // A fresh selection always starts from the stored decision (no override).
    void load(id, null);
  },
  { immediate: true },
);
</script>

<template>
  <section
    class="view"
    aria-label="Message"
  >
    <EmptyState
      v-if="state === 'idle'"
      title="No message selected"
      detail="Select a message from the list to read it here."
    />
    <p
      v-else-if="state === 'loading'"
      class="status"
      role="status"
    >
      Loading message…
    </p>
    <EmptyState
      v-else-if="state === 'error' || detail === null"
      title="Message unavailable"
      detail="This message could not be loaded. It may require the desktop app."
    />
    <MessageDetail
      v-else
      :detail="detail"
      @show-remote-once="showRemoteOnce"
      @always-allow-remote="alwaysAllowRemote"
    />
  </section>
</template>

<style scoped>
.view {
  height: 100%;
  overflow-y: auto;
  background: var(--color-bg);
}

.status {
  padding: var(--space-5);
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
}
</style>
