<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { MessageDetailDto, RsvpChoice } from "../../types/mail";
import { formatDateTime, formatRecipients } from "./format";
import InvitationCard from "./InvitationCard.vue";
import MessageActionBar from "./MessageActionBar.vue";
import MessageActions from "./MessageActions.vue";
import MessageAttachments from "./MessageAttachments.vue";
import MessageBody from "./MessageBody.vue";
import MessageReceiptBar from "./MessageReceiptBar.vue";
import MessageTagArea from "./MessageTagArea.vue";
import PgpBadge from "./PgpBadge.vue";

const { t } = useI18n({ useScope: "global" });

const props = withDefaults(
  defineProps<{
    detail: MessageDetailDto;
    /** True while an RSVP request is in flight. */
    rsvpBusy?: boolean;
    /** A redacted failure reason from the last RSVP attempt, if any. */
    rsvpError?: string | null;
    /** The RSVP stored but its reply failed to send — offer a resend. */
    rsvpSendFailed?: boolean;
    /** The RSVP bridge is unavailable (browser preview). */
    rsvpUnavailable?: boolean;
  }>(),
  {
    rsvpBusy: false,
    rsvpError: null,
    rsvpSendFailed: false,
    rsvpUnavailable: false,
  },
);

const emit = defineEmits<{
  (event: "show-remote-once"): void;
  (event: "always-allow-remote", sender: string): void;
  (event: "toggle-star"): void;
  (event: "print"): void;
  (event: "reply"): void;
  (event: "reply-all"): void;
  (event: "forward"): void;
  (event: "move"): void;
  (event: "copy"): void;
  (event: "archive"): void;
  (event: "junk"): void;
  (event: "delete"): void;
  (event: "rsvp", choice: RsvpChoice): void;
  (event: "retry-rsvp"): void;
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
    <MessageActionBar
      @reply="emit('reply')"
      @reply-all="emit('reply-all')"
      @forward="emit('forward')"
      @archive="emit('archive')"
      @move="emit('move')"
      @copy="emit('copy')"
      @junk="emit('junk')"
      @delete="emit('delete')"
    />

    <header class="header">
      <div class="subject-row">
        <h1 class="subject">
          {{ detail.subject ?? t("message.row.noSubject") }}
        </h1>
        <button
          type="button"
          class="star"
          :class="{ on: detail.flagged }"
          :aria-pressed="detail.flagged"
          :aria-label="detail.flagged ? t('message.detail.removeStar') : t('message.detail.addStar')"
          @click="emit('toggle-star')"
        >
          <span aria-hidden="true">{{ detail.flagged ? "★" : "☆" }}</span>
        </button>
      </div>
      <p class="from">
        <span class="from-name">{{ detail.fromName || detail.fromAddress }}</span>
        <span class="from-address">&lt;{{ detail.fromAddress }}&gt;</span>
      </p>
      <dl class="fields">
        <template v-if="toLine">
          <dt>{{ t("message.detail.to") }}</dt>
          <dd>{{ toLine }}</dd>
        </template>
        <template v-if="ccLine">
          <dt>{{ t("message.detail.cc") }}</dt>
          <dd>{{ ccLine }}</dd>
        </template>
        <template v-if="dateLine">
          <dt>{{ t("message.detail.date") }}</dt>
          <dd>{{ dateLine }}</dd>
        </template>
      </dl>
      <MessageTagArea :message-id="detail.id" />
    </header>

    <PgpBadge :message-id="detail.id" />

    <InvitationCard
      v-if="detail.invitation"
      :invitation="detail.invitation"
      :busy="rsvpBusy"
      :error="rsvpError"
      :send-failed="rsvpSendFailed"
      :unavailable="rsvpUnavailable"
      @rsvp="(choice) => emit('rsvp', choice)"
      @retry-send="emit('retry-rsvp')"
    />

    <MessageReceiptBar
      v-if="detail.receiptRequest"
      :key="detail.id"
      :receipt="detail.receiptRequest"
      :message-id="detail.id"
    />

    <MessageActions
      :message-id="detail.id"
      :subject="detail.subject"
      @print="emit('print')"
    />

    <MessageAttachments
      v-if="detail.attachments.length > 0"
      :message-id="detail.id"
      :attachments="detail.attachments"
    />

    <div
      v-if="detail.scamFlags.length > 0"
      class="scam-bar"
      role="alert"
    >
      <p class="scam-title">
        {{ t("message.detail.scamTitle") }}
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
      <span class="info-text">{{ t("message.detail.remoteBlocked", { count: detail.remoteBlockedCount }) }}</span>
      <span class="info-actions">
        <button
          type="button"
          class="bar-action"
          @click="emit('show-remote-once')"
        >
          {{ t("message.detail.showOnce") }}
        </button>
        <button
          type="button"
          class="bar-action"
          :disabled="!detail.fromAddress"
          @click="alwaysAllow"
        >
          {{ t("message.detail.alwaysAllowFrom", { sender: detail.fromAddress || t("message.detail.thisSender") }) }}
        </button>
      </span>
    </div>

    <p
      v-if="detail.hadActiveContent"
      class="notice"
      role="note"
    >
      {{ t("message.detail.activeContentRemoved") }}
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

.subject-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-2);
}

.subject {
  font-size: var(--text-lg);
  font-weight: var(--weight-semibold);
  letter-spacing: -0.01em;
}

.star {
  flex-shrink: 0;
  font-size: var(--text-lg);
  line-height: 1;
  color: var(--color-text-tertiary);
  transition: color var(--transition-fast);
}

.star:hover {
  color: var(--color-text-secondary);
}

.star.on {
  color: var(--color-accent);
}

.star:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: 2px;
  border-radius: var(--radius-sm);
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
