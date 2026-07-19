<script setup lang="ts">
import { computed } from "vue";

import type { MessageDetailDto } from "../../types/mail";
import { formatBytes, formatDateTime, formatRecipients } from "./format";
import MessageBody from "./MessageBody.vue";
import MessageTagArea from "./MessageTagArea.vue";

const props = defineProps<{
  detail: MessageDetailDto;
}>();

const emit = defineEmits<{
  (event: "show-remote-once"): void;
  (event: "always-allow-remote", sender: string): void;
}>();

const toLine = computed(() => formatRecipients(props.detail.to));
const ccLine = computed(() => formatRecipients(props.detail.cc));
const dateLine = computed(() => formatDateTime(props.detail.dateUnix));

// Offer the remote-content controls only when something was actually blocked
// and it is not already being shown for this render.
const remoteBlocked = computed(
  () => props.detail.remoteBlockedCount > 0 && !props.detail.remoteAllowed,
);

function alwaysAllow(): void {
  if (props.detail.fromAddress) {
    emit("always-allow-remote", props.detail.fromAddress);
  }
}
</script>

<template>
  <article class="detail">
    <header class="header">
      <h1 class="subject">
        {{ detail.subject ?? "(no subject)" }}
      </h1>
      <p class="from">
        <span class="from-name">{{ detail.fromName || detail.fromAddress }}</span>
        <span class="from-address">&lt;{{ detail.fromAddress }}&gt;</span>
      </p>
      <dl class="fields">
        <template v-if="toLine">
          <dt>To</dt>
          <dd>{{ toLine }}</dd>
        </template>
        <template v-if="ccLine">
          <dt>Cc</dt>
          <dd>{{ ccLine }}</dd>
        </template>
        <template v-if="dateLine">
          <dt>Date</dt>
          <dd>{{ dateLine }}</dd>
        </template>
      </dl>
      <MessageTagArea :message-id="detail.id" />
    </header>

    <ul
      v-if="detail.attachments.length > 0"
      class="attachments"
      aria-label="Attachments"
    >
      <li
        v-for="(attachment, index) in detail.attachments"
        :key="index"
        class="chip"
        title="Saving attachments arrives later"
      >
        <span class="chip-name">{{ attachment.filename }}</span>
        <span class="chip-size">{{ formatBytes(attachment.size) }}</span>
      </li>
    </ul>

    <div
      v-if="detail.scamFlags.length > 0"
      class="scam-bar"
      role="alert"
    >
      <p class="scam-title">
        This message looks like a possible scam
      </p>
      <ul class="scam-reasons">
        <li
          v-for="(flag, index) in detail.scamFlags"
          :key="index"
        >
          {{ flag.detail }}
        </li>
      </ul>
    </div>

    <div
      v-if="remoteBlocked"
      class="info-bar"
      role="status"
    >
      <span class="info-text">Remote content blocked ({{ detail.remoteBlockedCount }})</span>
      <span class="info-actions">
        <button
          type="button"
          class="bar-action"
          @click="emit('show-remote-once')"
        >
          Show once
        </button>
        <button
          type="button"
          class="bar-action"
          :disabled="!detail.fromAddress"
          @click="alwaysAllow"
        >
          Always allow from {{ detail.fromAddress || "this sender" }}
        </button>
      </span>
    </div>

    <p
      v-if="detail.hadActiveContent"
      class="notice"
      role="note"
    >
      Active content was removed for your safety.
    </p>

    <MessageBody
      :text-body="detail.textBody"
      :sanitized-html="detail.sanitizedHtml"
    />
  </article>
</template>

<style scoped>
.detail {
  padding: var(--space-5);
}

.header {
  border-bottom: 1px solid var(--color-border);
  padding-bottom: var(--space-3);
}

.subject {
  font-size: var(--text-lg);
  font-weight: var(--weight-semibold);
  letter-spacing: -0.01em;
}

.from {
  margin-top: var(--space-1);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.from-address {
  color: var(--color-text-tertiary);
  margin-left: var(--space-1);
}

.fields {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 2px var(--space-2);
  margin-top: var(--space-2);
  font-size: var(--text-sm);
}

.fields dt {
  color: var(--color-text-tertiary);
  font-weight: var(--weight-medium);
}

.fields dd {
  color: var(--color-text-secondary);
  margin: 0;
  overflow-wrap: anywhere;
}

.attachments {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  margin-top: var(--space-3);
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
  cursor: default;
}

.chip-size {
  color: var(--color-text-tertiary);
}

.notice {
  margin-top: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--color-bg-subtle);
  border: 1px solid var(--color-border);
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

/* Informational bar: remote content was blocked, offering the controls. */
.info-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  margin-top: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--color-accent-subtle);
  border: 1px solid var(--color-border);
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.info-actions {
  display: inline-flex;
  gap: var(--space-2);
}

.bar-action {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  font-size: var(--text-xs);
  color: var(--color-accent);
  cursor: pointer;
}

.bar-action:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.bar-action:disabled {
  color: var(--color-text-tertiary);
  cursor: default;
}

/*
 * Scam warning bar: distinct danger styling and no dismiss control, so the
 * warning stays visible while the message is open.
 */
.scam-bar {
  margin-top: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-danger);
  border-left-width: 3px;
  border-radius: var(--radius-sm);
  background: var(--color-bg-subtle);
}

.scam-title {
  font-size: var(--text-sm);
  font-weight: var(--weight-semibold);
  color: var(--color-danger);
}

.scam-reasons {
  margin: var(--space-1) 0 0;
  padding-left: var(--space-4);
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.scam-reasons li {
  overflow-wrap: anywhere;
}
</style>
