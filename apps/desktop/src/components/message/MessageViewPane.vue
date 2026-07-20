<script setup lang="ts">
import { ref, watch } from "vue";

import { respondInvitation } from "../../api/invitations";
import {
  getMessageDetail,
  setMessageFlagged,
  setRemoteContentAllowed,
} from "../../api/messages";
import { useCommandRegistry } from "../../composables/useCommandRegistry";
import { printMessageDocument } from "../../lib/printDocument";
import type { MessageDetailDto, RsvpChoice } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";
import MessageDetail from "./MessageDetail.vue";

const props = defineProps<{
  messageId: number | null;
  /**
   * True while a blocking modal is open. Gates the global Print shortcut so
   * Mod+P does not print the background message from behind a dialog. Optional
   * so an isolated unit mount defaults to "no modal". App.vue passes the real
   * predicate.
   */
  isModalOpen?: () => boolean;
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
// The RSVP was stored but the iMIP reply did not send: the card reflects the
// recorded status and offers "retry send" instead of a fresh Accept.
const rsvpSendFailed = ref(false);
// The RSVP bridge is absent (browser preview): show an unavailable state
// rather than silently dropping the click.
const rsvpUnavailable = ref(false);
// The last choice the user picked, so "retry send" re-sends it without asking.
const rsvpChoice = ref<RsvpChoice | null>(null);

/** Prints the open message as a sanitized, chrome-free document (ADR-0007). */
function printCurrent(): void {
  if (detail.value !== null && !detail.value.offlineUnavailable) {
    printMessageDocument(detail.value);
  }
}

// Global "Print message" command (Ctrl/Cmd+P); enabled only while a readable
// message is open AND no modal is covering it (so Mod+P never prints the
// background from behind a dialog). Auto-unregisters when this pane unmounts.
// Guarded so an isolated unit mount (no app-root command registry) still
// renders — the Print button keeps working there; only the global shortcut is
// skipped.
try {
  useCommandRegistry().register({
    id: "message.print",
    title: "Print message",
    keywords: ["print"],
    shortcut: "Mod+P",
    run: printCurrent,
    isEnabled: () =>
      !(props.isModalOpen?.() ?? false) &&
      detail.value !== null &&
      detail.value.offlineUnavailable !== true,
  });
} catch {
  // No command-registry ancestor: the pane is mounted in isolation (tests).
}

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
 *
 * The failure paths are distinct: a stored-but-not-sent response reflects the
 * recorded status and offers a resend (so a retry never re-accepts and
 * duplicates the RSVP), while a null result (no bridge) shows an unavailable
 * state instead of silently dropping the click.
 */
async function respond(choice: RsvpChoice): Promise<void> {
  const id = props.messageId;
  if (id === null || rsvpBusy.value) return;
  rsvpBusy.value = true;
  rsvpError.value = null;
  rsvpUnavailable.value = false;
  rsvpChoice.value = choice;
  try {
    const result = await respondInvitation(id, choice);
    if (props.messageId !== id) return;
    if (result === null) {
      rsvpUnavailable.value = true; // browser preview: no bridge
      return;
    }
    if (result.sent) {
      rsvpSendFailed.value = false;
      await load(id, null); // refresh so myStatus shows the response
      return;
    }
    if (result.myStatus !== "none") {
      // Stored, but the reply did not send. Reflect the recorded status on the
      // card so it stops offering a fresh Accept, and expose a retry-send.
      if (detail.value?.invitation) {
        detail.value.invitation.myStatus = result.myStatus;
      }
      rsvpSendFailed.value = true;
      rsvpError.value =
        result.error ?? "Your response was saved, but the reply could not be sent.";
    } else {
      // Nothing was stored: a plain failure the user can simply retry.
      rsvpSendFailed.value = false;
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

/** Re-sends the last RSVP after a send failure (the status is already stored). */
function retrySend(): void {
  if (rsvpChoice.value !== null) void respond(rsvpChoice.value);
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
    rsvpSendFailed.value = false;
    rsvpUnavailable.value = false;
    rsvpChoice.value = null;
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
      :rsvp-send-failed="rsvpSendFailed"
      :rsvp-unavailable="rsvpUnavailable"
      @show-remote-once="showRemoteOnce"
      @always-allow-remote="alwaysAllowRemote"
      @toggle-star="toggleStar"
      @rsvp="respond"
      @retry-rsvp="retrySend"
      @print="printCurrent"
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
