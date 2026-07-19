<script setup lang="ts">
/**
 * Account-management settings section.
 *
 * Lists every configured account (name, email, protocol) and hosts the edit
 * and delete flows. It loads its own data through `list_account_details` and
 * reloads after every mutation, so it never depends on the surrounding shell
 * for freshness. Deletion goes through {@link DeleteAccountDialog}, which
 * requires typing the account email before the destructive, local-only
 * removal can proceed.
 */
import { onMounted, ref } from "vue";

import { deleteAccount, listAccountDetails } from "../../api/accounts";
import type { AccountDetail } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";
import AccountEditForm from "./AccountEditForm.vue";
import DeleteAccountDialog from "./DeleteAccountDialog.vue";

type LoadState = "loading" | "ready" | "unavailable";

const state = ref<LoadState>("loading");
const accounts = ref<AccountDetail[]>([]);
const errorMessage = ref("");

const editing = ref<AccountDetail | null>(null);
const deleting = ref<AccountDetail | null>(null);
const deleteBusy = ref(false);
const deleteError = ref("");

async function load(): Promise<void> {
  const details = await listAccountDetails();
  if (details === null) {
    state.value = "unavailable";
    return;
  }
  accounts.value = details;
  state.value = "ready";
}

onMounted(load);

function messageFrom(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "The account could not be deleted.";
}

function onEdit(account: AccountDetail): void {
  errorMessage.value = "";
  editing.value = account;
}

function onEditCancel(): void {
  editing.value = null;
}

async function onSaved(): Promise<void> {
  editing.value = null;
  await load();
}

function onDelete(account: AccountDetail): void {
  deleteError.value = "";
  deleting.value = account;
}

function onDeleteCancel(): void {
  deleting.value = null;
}

async function onDeleteConfirm(): Promise<void> {
  if (deleting.value === null || deleteBusy.value) return;
  deleteBusy.value = true;
  try {
    await deleteAccount(deleting.value.id);
    deleting.value = null;
    await load();
  } catch (error) {
    deleteError.value = messageFrom(error);
  } finally {
    deleteBusy.value = false;
  }
}

function protocols(account: AccountDetail): string {
  return `${account.incoming.protocol.toUpperCase()} · SMTP`;
}
</script>

<template>
  <div class="accounts">
    <p
      v-if="state === 'loading'"
      class="note"
      role="status"
    >
      Loading accounts…
    </p>
    <p
      v-else-if="state === 'unavailable'"
      class="note"
      role="status"
    >
      Account management needs the desktop app; it is unavailable in the browser
      preview.
    </p>

    <AccountEditForm
      v-else-if="editing !== null"
      :account="editing"
      @saved="onSaved"
      @cancel="onEditCancel"
    />

    <EmptyState
      v-else-if="accounts.length === 0"
      title="No accounts yet"
      detail="Add an account from the sidebar; it will appear here to edit or remove."
    />

    <ul
      v-else
      class="list"
      aria-label="Configured accounts"
    >
      <li
        v-for="account in accounts"
        :key="account.id"
        class="account"
      >
        <div class="meta">
          <span class="name">{{ account.displayName }}</span>
          <span class="email">{{ account.email }}</span>
          <span class="protocols">{{ protocols(account) }}</span>
        </div>
        <div class="row-actions">
          <button
            type="button"
            class="button"
            @click="onEdit(account)"
          >
            Edit
          </button>
          <button
            type="button"
            class="button danger-text"
            @click="onDelete(account)"
          >
            Delete
          </button>
        </div>
      </li>
    </ul>

    <p
      class="error"
      role="alert"
      aria-live="assertive"
    >
      {{ errorMessage }}
    </p>
  </div>

  <DeleteAccountDialog
    v-if="deleting !== null"
    :display-name="deleting.displayName"
    :email="deleting.email"
    :deleting="deleteBusy"
    :error-message="deleteError"
    @confirm="onDeleteConfirm"
    @cancel="onDeleteCancel"
  />
</template>

<style scoped>
.accounts {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.note {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  list-style: none;
}

.account {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-3);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
}

.meta {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.name {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text);
}

.email {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
}

.protocols {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.row-actions {
  display: flex;
  gap: var(--space-2);
  flex-shrink: 0;
}

.button {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  padding: var(--space-1) var(--space-3);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-strong);
  color: var(--color-text);
  background: var(--color-bg);
  transition: background var(--transition-fast);
}

.button:hover {
  background: var(--color-bg-hover);
}

.button.danger-text {
  color: var(--color-danger);
  border-color: var(--color-border);
}

.error {
  font-size: var(--text-sm);
  color: var(--color-danger);
}

.error:empty {
  display: none;
}
</style>
