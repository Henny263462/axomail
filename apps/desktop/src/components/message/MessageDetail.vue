<script setup lang="ts">
import { computed } from "vue";

import type { MessageDetailDto } from "../../types/mail";
import { formatBytes, formatDateTime, formatRecipients } from "./format";
import MessageBody from "./MessageBody.vue";

const props = defineProps<{
  detail: MessageDetailDto;
}>();

const toLine = computed(() => formatRecipients(props.detail.to));
const ccLine = computed(() => formatRecipients(props.detail.cc));
const dateLine = computed(() => formatDateTime(props.detail.dateUnix));
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

    <p
      v-if="detail.remoteBlocked > 0"
      class="notice"
      role="note"
    >
      {{ detail.remoteBlocked }} remote resources blocked
    </p>
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
</style>
