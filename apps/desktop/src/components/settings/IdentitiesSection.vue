<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import {
  createIdentity,
  deleteIdentity,
  listIdentities,
  setDefaultIdentity,
  updateIdentity,
} from "../../api/identities";
import type { IdentityDto, IdentityInput } from "../../types/mail";
import type { SettingsSectionContext } from "./context";
import IdentityForm from "./IdentityForm.vue";

// A registry section: its only prop is the shared context. Identities are
// managed autonomously (loaded and mutated through the identities api); the
// context supplies the account list this section groups identities under.
defineProps<{ context: SettingsSectionContext }>();

const { t } = useI18n({ useScope: "global" });

const identities = ref<IdentityDto[]>([]);
type LoadState = "loading" | "ready" | "unavailable";
const state = ref<LoadState>("loading");
const errorMessage = ref("");

// The open editor, if any: which account it targets and the identity being
// edited (null for a new one).
const editing = ref<{ accountId: number; identity: IdentityDto | null } | null>(null);

onMounted(reload);

async function reload(): Promise<void> {
  const loaded = await listIdentities();
  if (loaded === null) {
    state.value = "unavailable";
    return;
  }
  identities.value = loaded;
  state.value = "ready";
}

/** Identities belonging to one account, default first. */
function forAccount(accountId: number): IdentityDto[] {
  return identities.value.filter((identity) => identity.accountId === accountId);
}

function openAdd(accountId: number): void {
  errorMessage.value = "";
  editing.value = { accountId, identity: null };
}

function openEdit(identity: IdentityDto): void {
  errorMessage.value = "";
  editing.value = { accountId: identity.accountId, identity };
}

function closeForm(): void {
  editing.value = null;
}

function messageFrom(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return t("settings.identities.saveFailed");
}

async function onSave(input: IdentityInput): Promise<void> {
  const target = editing.value;
  if (target === null) return;
  try {
    if (target.identity === null) {
      await createIdentity(input);
    } else {
      await updateIdentity(target.identity.id, input);
    }
    editing.value = null;
    await reload();
  } catch (error) {
    errorMessage.value = messageFrom(error);
  }
}

async function onDelete(identity: IdentityDto): Promise<void> {
  await deleteIdentity(identity.id);
  await reload();
}

async function onSetDefault(identity: IdentityDto): Promise<void> {
  try {
    await setDefaultIdentity(identity.id);
    await reload();
  } catch (error) {
    errorMessage.value = messageFrom(error);
  }
}
</script>

<template>
  <section
    class="identities"
    aria-labelledby="identities-heading"
  >
    <h3
      id="identities-heading"
      class="heading"
    >
      {{ t("settings.identities.title") }}
    </h3>

    <p
      v-if="state === 'loading'"
      class="note"
      role="status"
    >
      {{ t("settings.identities.loading") }}
    </p>
    <p
      v-else-if="state === 'unavailable'"
      class="note"
      role="status"
    >
      {{ t("settings.identities.unavailable") }}
    </p>
    <p
      v-else-if="context.accounts.length === 0"
      class="note"
      role="status"
    >
      {{ t("settings.identities.noAccounts") }}
    </p>

    <template v-else>
      <div
        v-for="account in context.accounts"
        :key="account.id"
        class="account"
      >
        <h4 class="account-name">
          {{ account.displayName }}
        </h4>
        <ul class="identity-list">
          <li
            v-for="identity in forAccount(account.id)"
            :key="identity.id"
            class="identity"
          >
            <div class="identity-main">
              <span class="identity-email">{{ identity.email }}</span>
              <span
                v-if="identity.isDefault"
                class="badge"
              >{{ t("settings.identities.default") }}</span>
            </div>
            <div class="identity-actions">
              <button
                v-if="!identity.isDefault"
                type="button"
                class="link"
                @click="onSetDefault(identity)"
              >
                {{ t("settings.identities.makeDefault") }}
              </button>
              <button
                type="button"
                class="link"
                @click="openEdit(identity)"
              >
                {{ t("common.edit") }}
              </button>
              <button
                v-if="!identity.isDefault"
                type="button"
                class="link danger"
                @click="onDelete(identity)"
              >
                {{ t("common.remove") }}
              </button>
            </div>
          </li>
        </ul>

        <IdentityForm
          v-if="editing?.accountId === account.id"
          :account-id="account.id"
          :identity="editing.identity"
          @save="onSave"
          @cancel="closeForm"
        />
        <button
          v-else
          type="button"
          class="add"
          @click="openAdd(account.id)"
        >
          {{ t("settings.identities.addIdentity") }}
        </button>
      </div>
    </template>

    <p
      class="error"
      role="alert"
      aria-live="assertive"
    >
      {{ errorMessage }}
    </p>
  </section>
</template>

<style scoped>
.identities {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.heading {
  font-size: var(--text-sm);
  font-weight: var(--weight-semibold);
  color: var(--color-text);
}

.note {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.account {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.account-name {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.identity-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.identity {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
}

.identity-main {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}

.identity-email {
  font-size: var(--text-sm);
  color: var(--color-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.badge {
  flex-shrink: 0;
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  padding: 1px var(--space-2);
  border-radius: var(--radius-sm);
  color: var(--color-accent);
  border: 1px solid var(--color-accent);
}

.identity-actions {
  display: flex;
  gap: var(--space-2);
  flex-shrink: 0;
}

.link {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-accent);
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
}

.link.danger {
  color: var(--color-danger);
}

.add {
  align-self: flex-start;
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text);
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  background: var(--color-bg);
}

.add:hover {
  background: var(--color-bg-hover);
}

.error {
  font-size: var(--text-sm);
  color: var(--color-danger);
}

.error:empty {
  display: none;
}
</style>
