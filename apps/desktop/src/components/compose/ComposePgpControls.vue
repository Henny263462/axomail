<script setup lang="ts">
/**
 * Composer OpenPGP toggles (Track 5.1): Sign and Encrypt.
 *
 * It queries the backend for what is possible given the sender and the current
 * recipients (`pgp_compose_status`) and reflects that live: signing is offered
 * only when an own key exists, encryption only when every recipient has a known
 * public key — otherwise the Encrypt toggle is disabled and the missing
 * recipients are named. A signing-key passphrase field appears while signing is
 * on; its value is passed to the send and never stored.
 */
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import { composeStatus } from "../../api/pgp";

const props = defineProps<{
  /** The sender's email (drives the signing-key lookup). */
  fromEmail: string;
  /** Current recipient address strings (To + Cc + Bcc). */
  recipients: string[];
  sign: boolean;
  encrypt: boolean;
  passphrase: string;
}>();

const emit = defineEmits<{
  "update:sign": [value: boolean];
  "update:encrypt": [value: boolean];
  "update:passphrase": [value: string];
}>();

const { t } = useI18n({ useScope: "global" });

const canSign = ref(false);
const canEncrypt = ref(false);
const missing = ref<string[]>([]);

// Monotonic guard: recipient-key lookups can resolve out of order, so a lookup
// for an earlier recipient set must never clobber a newer one. Each refresh
// takes a token and only the latest applies — Encrypt always reflects the LATEST
// recipients, never a stale "canEncrypt:true" for a set that lacked a key.
let lookupToken = 0;

/** Re-evaluates availability whenever the sender or recipients change. */
async function refresh(): Promise<void> {
  const token = ++lookupToken;
  const status = await composeStatus(props.fromEmail, props.recipients);
  // A newer recipient set was requested while this was in flight: discard.
  if (token !== lookupToken) return;
  if (status === null) {
    canSign.value = false;
    canEncrypt.value = false;
    missing.value = [];
    return;
  }
  canSign.value = status.canSign;
  canEncrypt.value = status.canEncrypt;
  missing.value = status.missingRecipients;
  // A no-longer-possible choice is switched off so a message is never sent with
  // a stale, unsatisfiable protection request.
  if (!canSign.value && props.sign) emit("update:sign", false);
  if (!canEncrypt.value && props.encrypt) emit("update:encrypt", false);
}

watch(
  () => [props.fromEmail, props.recipients.join(",")],
  refresh,
  { immediate: true },
);
</script>

<template>
  <fieldset
    v-if="canSign || canEncrypt || sign || encrypt"
    class="pgp-controls"
  >
    <legend class="legend">
      {{ t("pgp.title") }}
    </legend>
    <label class="toggle">
      <input
        type="checkbox"
        :checked="sign"
        :disabled="!canSign"
        @change="emit('update:sign', ($event.target as HTMLInputElement).checked)"
      >
      <span>
        <span class="toggle-label">{{ t("pgp.sign") }}</span>
        <span class="toggle-hint">{{ t("pgp.signHint") }}</span>
      </span>
    </label>
    <label class="toggle">
      <input
        type="checkbox"
        :checked="encrypt"
        :disabled="!canEncrypt"
        @change="emit('update:encrypt', ($event.target as HTMLInputElement).checked)"
      >
      <span>
        <span class="toggle-label">{{ t("pgp.encrypt") }}</span>
        <span class="toggle-hint">{{ t("pgp.encryptHint") }}</span>
      </span>
    </label>
    <p
      v-if="!canEncrypt && missing.length > 0"
      class="missing"
    >
      {{ t("pgp.encryptUnavailable", { missing: missing.join(", ") }) }}
    </p>
    <label
      v-if="sign"
      class="passphrase"
    >
      <span class="toggle-label">{{ t("pgp.signPassphrase") }}</span>
      <input
        type="password"
        :value="passphrase"
        :aria-label="t('pgp.signPassphrase')"
        @input="emit('update:passphrase', ($event.target as HTMLInputElement).value)"
      >
    </label>
  </fieldset>
</template>

<style scoped>
.pgp-controls {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-2);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.legend {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 0 var(--space-1);
}

.toggle {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  font-size: var(--text-sm);
}

.toggle-label {
  display: block;
  font-weight: var(--weight-medium);
}

.toggle-hint {
  display: block;
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.missing {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.passphrase {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  font-size: var(--text-sm);
}
</style>
