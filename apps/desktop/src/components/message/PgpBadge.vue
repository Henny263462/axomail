<script setup lang="ts">
/**
 * OpenPGP status badge for the reading pane (Track 5.1).
 *
 * On opening a message it asks the backend (`analyze_message_pgp`) whether the
 * message is signed and/or encrypted, and shows a verified/invalid/unknown-key
 * signature badge. For an encrypted message it offers a Decrypt action; if the
 * secret key is passphrase-protected the user enters the passphrase here (it is
 * sent for this call only, never stored). A successfully decrypted plaintext
 * body is shown as text — the sensitive content never re-enters the HTML render
 * path, only the backend-sanitized values cross the boundary.
 */
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import { analyzeMessagePgp } from "../../api/pgp";
import type { PgpMessageStatusDto } from "../../types/pgp";

const props = defineProps<{ messageId: number }>();
const { t } = useI18n({ useScope: "global" });

const status = ref<PgpMessageStatusDto | null>(null);
const passphrase = ref("");
const busy = ref(false);

async function analyze(pass: string | null): Promise<void> {
  busy.value = true;
  try {
    status.value = await analyzeMessagePgp(props.messageId, pass);
  } finally {
    busy.value = false;
  }
}

watch(
  () => props.messageId,
  (id) => {
    status.value = null;
    passphrase.value = "";
    if (Number.isFinite(id)) void analyze(null);
  },
  { immediate: true },
);

function onDecrypt(): void {
  void analyze(passphrase.value.length > 0 ? passphrase.value : null);
}

/** The signature chip's label + tone, or null when unsigned. */
function signatureChip(
  s: PgpMessageStatusDto,
): { label: string; tone: string } | null {
  if (!s.signed) return null;
  switch (s.signatureStatus) {
    case "valid":
      return { label: t("pgp.badge.signedValid"), tone: "ok" };
    case "invalid":
      return { label: t("pgp.badge.signedInvalid"), tone: "bad" };
    default:
      return { label: t("pgp.badge.signedUnknown"), tone: "warn" };
  }
}
</script>

<template>
  <section
    v-if="status && status.isPgp"
    class="pgp-badge"
    role="status"
  >
    <div class="chips">
      <span
        v-if="status.encrypted"
        class="chip ok"
      >{{
        status.decrypted ? t("pgp.badge.decrypted") : t("pgp.badge.encrypted")
      }}</span>
      <span
        v-if="signatureChip(status)"
        class="chip"
        :class="signatureChip(status)?.tone"
      >{{ signatureChip(status)?.label }}</span>
    </div>

    <div
      v-if="status.encrypted && !status.decrypted && status.needsPassphrase"
      class="unlock"
    >
      <label class="unlock-label">{{ t("pgp.badge.needsPassphrase") }}</label>
      <div class="unlock-row">
        <input
          v-model="passphrase"
          type="password"
          :aria-label="t('pgp.badge.needsPassphrase')"
          @keydown.enter.prevent="onDecrypt"
        >
        <button
          type="button"
          :disabled="busy"
          @click="onDecrypt"
        >
          {{ t("pgp.badge.decrypt") }}
        </button>
      </div>
    </div>

    <p
      v-else-if="status.encrypted && !status.decrypted && status.error"
      class="error"
    >
      {{ t("pgp.badge.failed") }}
    </p>

    <pre
      v-if="status.decrypted && status.bodyText"
      class="decrypted-body"
    >{{ status.bodyText }}</pre>
  </section>
</template>

<style scoped>
.pgp-badge {
  margin-top: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-bg-subtle);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
}

.chip {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  padding: 0 var(--space-2);
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border-strong);
  color: var(--color-text-secondary);
}

.chip.ok {
  color: var(--color-accent);
  border-color: var(--color-accent);
}

.chip.bad {
  color: var(--color-danger);
  border-color: var(--color-danger);
}

.chip.warn {
  color: var(--color-text-secondary);
}

.unlock {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.unlock-label {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.unlock-row {
  display: flex;
  gap: var(--space-2);
}

.unlock-row input {
  flex: 1 1 auto;
  font-size: var(--text-sm);
}

.error {
  font-size: var(--text-xs);
  color: var(--color-danger);
}

.decrypted-body {
  margin: 0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-size: var(--text-sm);
  color: var(--color-text);
}
</style>
