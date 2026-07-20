<script setup lang="ts">
/**
 * The "OpenPGP encryption" settings section (Track 5.1): generate/import/export
 * our own keys and manage recipient public keys.
 *
 * An autonomous registry section: its only prop is the shared
 * {@link SettingsSectionContext}. It loads keys on mount and persists every
 * action through its own `api/pgp` calls, reporting outcomes through the shared
 * aria-live regions. Secret key material never leaves the backend except as the
 * user-initiated armored backup, which is shown for the user to save.
 */
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import {
  deleteOwnKey,
  deleteRecipientKey,
  exportPublicKey,
  exportSecretKey,
  generateKey,
  importRecipientKey,
  importSecretKey,
  listOwnKeys,
  listRecipientKeys,
  setDefaultKey,
} from "../../api/pgp";
import type { PgpKeyDto, PgpRecipientKeyDto } from "../../types/pgp";
import { messageFrom, type SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();
const { t } = useI18n({ useScope: "global" });

const ownKeys = ref<PgpKeyDto[]>([]);
const recipientKeys = ref<PgpRecipientKeyDto[]>([]);
const busy = ref(false);

const newUserId = ref("");
const newPassphrase = ref("");
const importArmor = ref("");
const recipientEmail = ref("");
const recipientArmor = ref("");
/** Armored key surfaced after an export, shown read-only for the user to copy. */
const exported = ref("");

onMounted(refresh);

async function refresh(): Promise<void> {
  const [own, recipients] = await Promise.all([
    listOwnKeys(),
    listRecipientKeys(),
  ]);
  if (own !== null) ownKeys.value = own;
  if (recipients !== null) recipientKeys.value = recipients;
}

/** Runs a mutating action with shared busy/error/status handling. */
async function run(action: () => Promise<void>): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  props.context.reportError("");
  props.context.reportStatus("");
  try {
    await action();
  } catch (error) {
    props.context.reportError(messageFrom(error));
  } finally {
    busy.value = false;
  }
}

function onGenerate(): Promise<void> {
  return run(async () => {
    // Read the passphrase transiently and wipe it from reactive state before
    // any await resolves, so it does not linger after the key is generated
    // (success or failure).
    const passphrase = newPassphrase.value;
    newPassphrase.value = "";
    try {
      const created = await generateKey(
        newUserId.value.trim(),
        passphrase.length > 0 ? passphrase : null,
      );
      if (created === null) return;
      newUserId.value = "";
      props.context.reportStatus(t("pgp.generate"));
      await refresh();
    } finally {
      newPassphrase.value = "";
    }
  });
}

function onImportSecret(): Promise<void> {
  return run(async () => {
    if (importArmor.value.trim().length === 0) return;
    await importSecretKey(importArmor.value);
    importArmor.value = "";
    await refresh();
  });
}

function onImportRecipient(): Promise<void> {
  return run(async () => {
    if (recipientArmor.value.trim().length === 0) return;
    await importRecipientKey(recipientEmail.value.trim(), recipientArmor.value);
    recipientEmail.value = "";
    recipientArmor.value = "";
    await refresh();
  });
}

function onMakeDefault(id: number): Promise<void> {
  return run(async () => {
    await setDefaultKey(id);
    await refresh();
  });
}

function onExportPublic(id: number): Promise<void> {
  return run(async () => {
    exported.value = (await exportPublicKey(id)) ?? "";
  });
}

function onExportSecret(id: number): Promise<void> {
  return run(async () => {
    exported.value = (await exportSecretKey(id)) ?? "";
  });
}

function onDeleteOwn(id: number): Promise<void> {
  return run(async () => {
    await deleteOwnKey(id);
    await refresh();
  });
}

function onDeleteRecipient(id: number): Promise<void> {
  return run(async () => {
    await deleteRecipientKey(id);
    await refresh();
  });
}
</script>

<template>
  <div class="pgp">
    <p class="hint">
      {{ t("pgp.hint") }}
    </p>

    <fieldset class="group">
      <legend class="legend">
        {{ t("pgp.ownKeys") }}
      </legend>
      <p
        v-if="ownKeys.length === 0"
        class="empty"
      >
        {{ t("pgp.noOwnKeys") }}
      </p>
      <ul
        v-else
        class="keys"
      >
        <li
          v-for="key in ownKeys"
          :key="key.id"
          class="key"
        >
          <span class="key-main">
            <span class="key-user">{{ key.userId }}</span>
            <span class="key-fpr">{{ key.fingerprint }}</span>
          </span>
          <span class="tags">
            <span
              v-if="key.isDefault"
              class="tag"
            >{{ t("pgp.default") }}</span>
            <span
              v-if="key.hasPassphrase"
              class="tag"
            >{{ t("pgp.passphraseProtected") }}</span>
          </span>
          <span class="actions">
            <button
              v-if="!key.isDefault"
              type="button"
              :disabled="busy"
              @click="onMakeDefault(key.id)"
            >
              {{ t("pgp.makeDefault") }}
            </button>
            <button
              type="button"
              :disabled="busy"
              @click="onExportPublic(key.id)"
            >
              {{ t("pgp.exportPublic") }}
            </button>
            <button
              type="button"
              :disabled="busy"
              @click="onExportSecret(key.id)"
            >
              {{ t("pgp.exportSecret") }}
            </button>
            <button
              type="button"
              :disabled="busy"
              @click="onDeleteOwn(key.id)"
            >
              {{ t("pgp.delete") }}
            </button>
          </span>
        </li>
      </ul>

      <div class="form-row">
        <input
          v-model="newUserId"
          type="text"
          :placeholder="t('pgp.userIdLabel')"
          :aria-label="t('pgp.userIdLabel')"
        >
        <input
          v-model="newPassphrase"
          type="password"
          :placeholder="t('pgp.passphraseLabel')"
          :aria-label="t('pgp.passphraseLabel')"
        >
        <button
          type="button"
          :disabled="busy || newUserId.trim().length === 0"
          @click="onGenerate"
        >
          {{ t("pgp.generate") }}
        </button>
      </div>
      <div class="form-row">
        <textarea
          v-model="importArmor"
          rows="2"
          :placeholder="t('pgp.armorLabel')"
          :aria-label="t('pgp.armorLabel')"
        />
        <button
          type="button"
          :disabled="busy || importArmor.trim().length === 0"
          @click="onImportSecret"
        >
          {{ t("pgp.importSecret") }}
        </button>
      </div>
    </fieldset>

    <fieldset class="group">
      <legend class="legend">
        {{ t("pgp.recipientKeys") }}
      </legend>
      <p
        v-if="recipientKeys.length === 0"
        class="empty"
      >
        {{ t("pgp.noRecipientKeys") }}
      </p>
      <ul
        v-else
        class="keys"
      >
        <li
          v-for="key in recipientKeys"
          :key="key.id"
          class="key"
        >
          <span class="key-main">
            <span class="key-user">{{ key.email }}</span>
            <span class="key-fpr">{{ key.fingerprint }}</span>
          </span>
          <span class="actions">
            <button
              type="button"
              :disabled="busy"
              @click="onDeleteRecipient(key.id)"
            >
              {{ t("pgp.delete") }}
            </button>
          </span>
        </li>
      </ul>
      <div class="form-row">
        <input
          v-model="recipientEmail"
          type="email"
          :placeholder="t('pgp.emailLabel')"
          :aria-label="t('pgp.emailLabel')"
        >
        <textarea
          v-model="recipientArmor"
          rows="2"
          :placeholder="t('pgp.armorLabel')"
          :aria-label="t('pgp.armorLabel')"
        />
        <button
          type="button"
          :disabled="busy || recipientArmor.trim().length === 0"
          @click="onImportRecipient"
        >
          {{ t("pgp.importRecipient") }}
        </button>
      </div>
    </fieldset>

    <textarea
      v-if="exported.length > 0"
      class="exported"
      readonly
      rows="4"
      :value="exported"
      :aria-label="t('pgp.exportedAria')"
    />
  </div>
</template>

<style scoped>
.pgp {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.hint {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.group {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-3);
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

.empty {
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
}

.keys {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  list-style: none;
}

.key {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
  justify-content: space-between;
}

.key-main {
  display: flex;
  flex-direction: column;
}

.key-user {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
}

.key-fpr {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  font-family: var(--font-mono, monospace);
  word-break: break-all;
}

.tags {
  display: flex;
  gap: var(--space-1);
}

.tag {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  padding: 0 var(--space-1);
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
}

.form-row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: flex-start;
}

.form-row input,
.form-row textarea {
  flex: 1 1 12rem;
  font-size: var(--text-sm);
}

.exported {
  width: 100%;
  font-family: var(--font-mono, monospace);
  font-size: var(--text-xs);
}
</style>
