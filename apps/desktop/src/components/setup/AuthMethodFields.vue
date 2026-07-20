<script setup lang="ts">
import { useI18n } from "vue-i18n";

import type { AuthMechanism, OAuthProvider } from "../../types/mail";

const mechanism = defineModel<AuthMechanism>("mechanism", { required: true });
const provider = defineModel<OAuthProvider>("provider", { required: true });
const clientId = defineModel<string>("clientId", { required: true });
const authUrl = defineModel<string>("authUrl", { required: true });
const tokenUrl = defineModel<string>("tokenUrl", { required: true });
const scopes = defineModel<string>("scopes", { required: true });

const { t } = useI18n({ useScope: "global" });
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      {{ t("setup.auth.legend") }}
    </legend>

    <div class="field">
      <label for="auth-mechanism">{{ t("setup.auth.method") }}</label>
      <select
        id="auth-mechanism"
        v-model="mechanism"
      >
        <option value="password">
          {{ t("setup.auth.password") }}
        </option>
        <option value="oauth2">
          {{ t("setup.auth.oauth2") }}
        </option>
      </select>
    </div>

    <template v-if="mechanism === 'oauth2'">
      <div class="field">
        <label for="oauth-provider">{{ t("setup.auth.provider") }}</label>
        <select
          id="oauth-provider"
          v-model="provider"
        >
          <option value="gmail">
            Gmail
          </option>
          <option value="microsoft">
            Microsoft 365
          </option>
          <option value="custom">
            {{ t("setup.auth.providerCustom") }}
          </option>
        </select>
      </div>

      <div class="field">
        <label for="oauth-client-id">{{ t("setup.auth.clientId") }}</label>
        <input
          id="oauth-client-id"
          v-model="clientId"
          type="text"
          autocomplete="off"
        >
        <p class="hint">
          {{ t("setup.auth.clientIdHint") }}
        </p>
      </div>

      <template v-if="provider === 'custom'">
        <div class="field">
          <label for="oauth-auth-url">{{ t("setup.auth.authUrl") }}</label>
          <input
            id="oauth-auth-url"
            v-model="authUrl"
            type="url"
            autocomplete="off"
            placeholder="https://…"
          >
        </div>
        <div class="field">
          <label for="oauth-token-url">{{ t("setup.auth.tokenUrl") }}</label>
          <input
            id="oauth-token-url"
            v-model="tokenUrl"
            type="url"
            autocomplete="off"
            placeholder="https://…"
          >
        </div>
        <div class="field">
          <label for="oauth-scopes">{{ t("setup.auth.scopes") }}</label>
          <input
            id="oauth-scopes"
            v-model="scopes"
            type="text"
            autocomplete="off"
          >
        </div>
      </template>
    </template>
  </fieldset>
</template>

<style scoped>
.group {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
}

.legend {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 0 var(--space-1);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

label {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

.hint {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  line-height: 1.4;
}

input,
select {
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
  min-width: 0;
}

input:focus-visible,
select:focus-visible {
  border-color: var(--color-accent);
}
</style>
