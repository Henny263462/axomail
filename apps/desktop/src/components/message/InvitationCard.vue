<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { InvitationDto, RsvpChoice } from "../../types/mail";

const { t } = useI18n({ useScope: "global" });

const props = withDefaults(
  defineProps<{
    invitation: InvitationDto;
    /** True while an RSVP request is in flight (buttons disabled). */
    busy: boolean;
    /** A redacted failure reason from the last RSVP attempt, if any. */
    error: string | null;
    /**
     * The RSVP was stored but its reply failed to send. The card then reflects
     * the recorded status and offers a resend instead of a fresh Accept, so a
     * retry never re-records (and duplicates) the response.
     */
    sendFailed?: boolean;
    /** The RSVP bridge is unavailable (browser preview) — show a notice. */
    unavailable?: boolean;
  }>(),
  { sendFailed: false, unavailable: false },
);

const emit = defineEmits<{
  (event: "rsvp", choice: RsvpChoice): void;
  (event: "retry-send"): void;
}>();

const title = computed(() => props.invitation.summary ?? t("message.invitation.fallbackTitle"));

const organizer = computed(() => {
  const { organizerName, organizerEmail } = props.invitation;
  if (organizerName && organizerEmail) return `${organizerName} <${organizerEmail}>`;
  return organizerName ?? organizerEmail ?? t("message.invitation.unknownOrganizer");
});

/** The human-readable label for the user's current response. */
const responseLabel = computed(() => {
  switch (props.invitation.myStatus) {
    case "accepted":
      return t("message.invitation.acceptedResponse");
    case "tentative":
      return t("message.invitation.tentativeResponse");
    case "declined":
      return t("message.invitation.declinedResponse");
    default:
      return null;
  }
});

const choices: { choice: RsvpChoice; labelKey: string; status: string }[] = [
  { choice: "accept", labelKey: "message.invitation.accept", status: "accepted" },
  { choice: "tentative", labelKey: "message.invitation.tentative", status: "tentative" },
  { choice: "decline", labelKey: "message.invitation.decline", status: "declined" },
];
</script>

<template>
  <section
    class="invite"
    :aria-label="t('message.invitation.kicker')"
  >
    <p class="kicker">
      {{ t("message.invitation.kicker") }}
    </p>
    <h2 class="title">
      {{ title }}
    </h2>

    <dl class="fields">
      <dt>{{ t("message.invitation.organizer") }}</dt>
      <dd>{{ organizer }}</dd>
      <dt>{{ t("message.invitation.when") }}</dt>
      <dd>{{ invitation.whenDisplay ?? t("message.invitation.timeNotSpecified") }}</dd>
      <template v-if="invitation.location">
        <dt>{{ t("message.invitation.where") }}</dt>
        <dd>{{ invitation.location }}</dd>
      </template>
      <template v-if="invitation.description">
        <dt>{{ t("message.invitation.agenda") }}</dt>
        <dd class="agenda">
          {{ invitation.description }}
        </dd>
      </template>
    </dl>

    <p
      v-if="invitation.cancelled"
      class="cancelled"
      role="status"
    >
      {{ t("message.invitation.cancelled") }}
    </p>

    <div
      v-else-if="invitation.canRespond"
      class="respond"
    >
      <template v-if="sendFailed">
        <p
          v-if="responseLabel"
          class="response"
          role="status"
        >
          {{ responseLabel }}
        </p>
        <p
          class="send-failed"
          role="status"
        >
          {{ t("message.invitation.sendFailed") }}
        </p>
        <div
          class="buttons"
          role="group"
          :aria-label="t('message.invitation.retryGroupLabel')"
        >
          <button
            type="button"
            class="rsvp retry"
            :disabled="busy"
            @click="emit('retry-send')"
          >
            {{ t("message.invitation.retrySend") }}
          </button>
        </div>
      </template>
      <template v-else>
        <p
          class="conflict-note"
          role="note"
        >
          {{ t("message.invitation.conflictNote") }}
        </p>
        <div
          class="buttons"
          role="group"
          :aria-label="t('message.invitation.respondGroupLabel')"
        >
          <button
            v-for="option in choices"
            :key="option.choice"
            type="button"
            class="rsvp"
            :class="{ current: invitation.myStatus === option.status }"
            :aria-pressed="invitation.myStatus === option.status"
            :disabled="busy"
            @click="emit('rsvp', option.choice)"
          >
            {{ t(option.labelKey) }}
          </button>
        </div>
        <p
          v-if="responseLabel"
          class="response"
          role="status"
        >
          {{ responseLabel }}
        </p>
      </template>
    </div>

    <p
      v-else-if="responseLabel"
      class="response"
      role="status"
    >
      {{ responseLabel }}
    </p>

    <p
      v-if="unavailable"
      class="response"
      role="status"
    >
      {{ t("message.invitation.unavailable") }}
    </p>

    <p
      v-if="error"
      class="error"
      role="alert"
    >
      {{ error }}
    </p>
  </section>
</template>

<style scoped>
.invite {
  margin: var(--space-3) 0;
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--color-border);
  border-left: 3px solid var(--color-accent);
  border-radius: var(--radius-sm);
  background: var(--color-accent-subtle);
}

.kicker {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--color-accent);
}

.title {
  margin-top: var(--space-1);
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
  color: var(--color-text);
}

.fields {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 2px var(--space-3);
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

.agenda {
  white-space: pre-wrap;
}

.conflict-note {
  margin-top: var(--space-2);
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.buttons {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  margin-top: var(--space-2);
}

.rsvp {
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  font-size: var(--text-sm);
  color: var(--color-text);
  cursor: pointer;
}

.rsvp:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.rsvp:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: 2px;
}

.rsvp:disabled {
  color: var(--color-text-tertiary);
  cursor: default;
}

.rsvp.current {
  border-color: var(--color-accent);
  background: var(--color-accent);
  color: var(--color-text-inverse);
}

.response {
  margin-top: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.send-failed {
  margin-top: var(--space-2);
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-danger);
}

.cancelled {
  margin-top: var(--space-2);
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-danger);
}

.error {
  margin-top: var(--space-2);
  font-size: var(--text-xs);
  color: var(--color-danger);
}
</style>
