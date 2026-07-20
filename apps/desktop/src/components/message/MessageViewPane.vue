<script setup lang="ts">
import { ref, watch } from "vue";

import { respondInvitation } from "../../api/invitations";
import {
  getMessageDetail,
  setMessageFlagged,
  setRemoteContentAllowed,
} from "../../api/messages";
import type { MessageDetailDto, RsvpChoice } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";
import MessageDetail from "./MessageDetail.vue";

const props = defineProps<{
  messageId: number | null;
}>();

const emit = defineEmits<{
  /** The starred state changed here; the host syncs the list row. */
  flagChanged: [payload: { messageId: number; flagged: boolean }];
  /** Reply / reply-all / forward requested for the given message. */
  reply: [messageId: number];
  replyAll: [messageId: number];
  forward: [messageId: number];
  /** Move / copy / archive / delete-to-Trash requested for the given message. */
  move: [messageId: number];
  copy: [messageId: number];
  archive: [messageId: number];
  deleteMessage: [messageId: number];
  /** Mark the given message as junk (train + file to Junk). */
  junk: [messageId: number];
}>();

type LoadState = "idle" | "loading" | "loaded" | "error";

const state = ref<LoadState>("idle");
const detail = ref<MessageDetailDto | null>(null);
// RSVP request state for the invitation card, reset per selection.
const rsvpBusy = ref(false);
const rsvpError = ref<string | null>(null);

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

/**
 * Records an RSVP for the open invitation and sends the iMIP reply, then
 * reloads the detail so the stored response is reflected. A backend rejection
 * (offline, not a live request) or a failed send is surfaced on the card.
 */
async function respond(choice: RsvpChoice): Promise<void> {
  const id = props.messageId;
  if (id === null || rsvpBusy.value) return;
  rsvpBusy.value = true;
  rsvpError.value = null;
  try {
    const result = await respondInvitation(id, choice);
    if (props.messageId !== id) return;
    if (result === null) return; // browser preview: no bridge
    if (result.sent) {
      await load(id, null); // refresh so myStatus shows the response
    } else {
      rsvpError.value = result.error ?? "The response could not be sent.";
    }
  } catch (error) {
    if (props.messageId === id) {
      rsvpError.value =
        typeof error === "string" ? error : "The response could not be sent.";
    }
  } finally {
    if (props.messageId === id) rsvpBusy.value = false;
  }
}

/**
 * Optimistically toggles the open message's star and persists it, reverting
 * on a backend error. Notifies the host so the list row stays in step.
 */
async function toggleStar(): Promise<void> {
  const current = detail.value;
  if (current === null) return;
  const next = !current.flagged;
  current.flagged = next;
  try {
    await setMessageFlagged(current.id, next);
    emit("flagChanged", { messageId: current.id, flagged: next });
  } catch {
    current.flagged = !next;
  }
}

watch(
  () => props.messageId,
  (id) => {
    // A new selection clears any in-flight RSVP feedback.
    rsvpBusy.value = false;
    rsvpError.value = null;
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
    <EmptyState
      v-else-if="detail.offlineUnavailable"
      title="Not available offline"
      detail="This message has not been downloaded for offline reading. Reconnect to load it."
    />
    <MessageDetail
      v-else
      :detail="detail"
      :rsvp-busy="rsvpBusy"
      :rsvp-error="rsvpError"
      @show-remote-once="showRemoteOnce"
      @always-allow-remote="alwaysAllowRemote"
      @toggle-star="toggleStar"
      @rsvp="respond"
      @reply="emit('reply', detail.id)"
      @reply-all="emit('replyAll', detail.id)"
      @forward="emit('forward', detail.id)"
      @move="emit('move', detail.id)"
      @copy="emit('copy', detail.id)"
      @archive="emit('archive', detail.id)"
      @junk="emit('junk', detail.id)"
      @delete="emit('deleteMessage', detail.id)"
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
