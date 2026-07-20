<script setup lang="ts">
import { computed } from "vue";

import type { InvitationDto, RsvpChoice } from "../../types/mail";

const props = defineProps<{
  invitation: InvitationDto;
  /** True while an RSVP request is in flight (buttons disabled). */
  busy: boolean;
  /** A redacted failure reason from the last RSVP attempt, if any. */
  error: string | null;
}>();

const emit = defineEmits<{
  (event: "rsvp", choice: RsvpChoice): void;
}>();

const title = computed(() => props.invitation.summary ?? "Invitation");

const organizer = computed(() => {
  const { organizerName, organizerEmail } = props.invitation;
  if (organizerName && organizerEmail) return `${organizerName} <${organizerEmail}>`;
  return organizerName ?? organizerEmail ?? "Unknown organizer";
});

/** The human-readable label for the user's current response. */
const responseLabel = computed(() => {
  switch (props.invitation.myStatus) {
    case "accepted":
      return "You accepted this invitation.";
    case "tentative":
      return "You responded tentatively.";
    case "declined":
      return "You declined this invitation.";
    default:
      return null;
  }
});

const choices: { choice: RsvpChoice; label: string; status: string }[] = [
  { choice: "accept", label: "Accept", status: "accepted" },
  { choice: "tentative", label: "Tentative", status: "tentative" },
  { choice: "decline", label: "Decline", status: "declined" },
];
</script>

<template>
  <section
    class="invite"
    aria-label="Calendar invitation"
  >
    <p class="kicker">
      Calendar invitation
    </p>
    <h2 class="title">
      {{ title }}
    </h2>

    <dl class="fields">
      <dt>Organizer</dt>
      <dd>{{ organizer }}</dd>
      <dt>When</dt>
      <dd>{{ invitation.whenDisplay ?? "Time not specified" }}</dd>
      <template v-if="invitation.location">
        <dt>Where</dt>
        <dd>{{ invitation.location }}</dd>
      </template>
      <template v-if="invitation.description">
        <dt>Agenda</dt>
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
      This event has been cancelled by the organizer.
    </p>

    <div
      v-else-if="invitation.canRespond"
      class="respond"
    >
      <p
        class="conflict-note"
        role="note"
      >
        Conflicts are not checked — axomail keeps no calendar.
      </p>
      <div
        class="buttons"
        role="group"
        aria-label="Respond to invitation"
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
          {{ option.label }}
        </button>
      </div>
      <p
        v-if="responseLabel"
        class="response"
        role="status"
      >
        {{ responseLabel }}
      </p>
    </div>

    <p
      v-else-if="responseLabel"
      class="response"
      role="status"
    >
      {{ responseLabel }}
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
