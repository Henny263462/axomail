/**
 * Form state and submission logic for the account-setup dialog.
 *
 * Kept in a composable so the dialog component stays a thin view: it owns the
 * reactive fields for both the password and OAuth2 paths, validation, the live
 * connection test, and the account-first OAuth flow (create the row, then run
 * the browser sign-in and store tokens). No secret is ever held here beyond the
 * transient password the user types for a password account.
 */
import { onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import { createAccount, deleteAccount } from "../../api/accounts";
import { testConnection } from "../../api/connection";
import { oauthAuthorize, testAccountConnection } from "../../api/oauth";
import type {
  AccountSummary,
  AuthInput,
  AuthMechanism,
  ConnectionTestResult,
  CreateAccountInput,
  OAuthProvider,
} from "../../types/mail";
import type { ServerDraft } from "./draft";
import { useAutoconfigLookup } from "./useAutoconfigLookup";

/** Callbacks the dialog supplies so the composable can report completion. */
interface Callbacks {
  onCreated: (account: AccountSummary) => void;
}

export function useAccountSetup({ onCreated }: Callbacks) {
  // Global scope so the composable reads the same catalogue the dialog does.
  const { t } = useI18n({ useScope: "global" });

  const displayName = ref("");
  const email = ref("");

  const mechanism = ref<AuthMechanism>("password");
  const provider = ref<OAuthProvider>("gmail");
  const clientId = ref("");
  const customAuthUrl = ref("");
  const customTokenUrl = ref("");
  const customScopes = ref("");

  const incoming = ref<ServerDraft>({
    protocol: "imap",
    host: "",
    port: 993,
    security: "tls",
    username: "",
    password: "",
  });
  const outgoing = ref<ServerDraft>({
    host: "",
    port: 465,
    security: "tls",
    username: "",
    password: "",
  });

  const pending = ref(false);
  const errorMessage = ref("");
  const testing = ref(false);
  const testResult = ref<ConnectionTestResult | null>(null);
  const testMessage = ref("");

  // The "find settings automatically" step fills the fields above; kept in its
  // own composable so discovery stays separate from account creation.
  const { autoconfigStatus, autoconfigSource, onFindSettings } =
    useAutoconfigLookup({ email, incoming, outgoing, mechanism, provider });

  // Set once the OAuth2 account row exists, so a retry after a declined or
  // timed-out sign-in reuses the same account rather than creating another.
  const createdAccountId = ref<number | null>(null);
  const createdAccount = ref<AccountSummary | null>(null);
  // True only once setup has fully succeeded (account created AND, for OAuth2,
  // authorized). Guards {@link discardCreatedAccount} so a completed account is
  // never deleted by field-edit resets or dialog-close cleanup.
  const succeeded = ref(false);

  const isOAuth = () => mechanism.value === "oauth2";
  const isCustom = () => provider.value === "custom";

  /**
   * Best-effort removal of an OAuth2 account row that was created but never
   * authorized. Called (a) when the user edits any field after a failed
   * sign-in, so a retry re-creates the row with the edited config, and (b) when
   * the dialog closes without success, so no orphan account is left behind.
   *
   * A successful setup is never touched (guarded by {@link succeeded}), and a
   * double call is harmless: the id is cleared before the delete is awaited, so
   * a concurrent call returns immediately and the backend delete is idempotent.
   */
  async function discardCreatedAccount(): Promise<void> {
    const id = createdAccountId.value;
    if (id === null || succeeded.value) return;
    createdAccountId.value = null;
    createdAccount.value = null;
    try {
      await deleteAccount(id);
    } catch {
      // The row may already be gone; a failed cleanup must not surface an error.
    }
  }

  // Editing any field after an unauthorized OAuth2 row exists invalidates it:
  // drop the stale row now so the next submit re-creates it with the edited
  // client id / auth method / server values instead of silently reusing it.
  watch(
    [
      displayName,
      email,
      mechanism,
      provider,
      clientId,
      customAuthUrl,
      customTokenUrl,
      customScopes,
      incoming,
      outgoing,
    ],
    () => {
      if (createdAccountId.value !== null && !succeeded.value) {
        void discardCreatedAccount();
      }
    },
    { deep: true },
  );

  // Closing the dialog with a created-but-never-authorized OAuth2 account must
  // not leave an orphan row; a successful setup is guarded and never deleted.
  onBeforeUnmount(() => {
    void discardCreatedAccount();
  });

  function validate(): string | null {
    if (displayName.value.trim() === "") return t("setup.validation.displayName");
    const trimmedEmail = email.value.trim();
    if (trimmedEmail === "" || !trimmedEmail.includes("@")) {
      return t("setup.validation.email");
    }
    if (incoming.value.host.trim() === "") {
      return t("setup.validation.incomingHost");
    }
    if (incoming.value.username.trim() === "") {
      return t("setup.validation.incomingUsername");
    }
    if (outgoing.value.host.trim() === "") {
      return t("setup.validation.outgoingHost");
    }
    if (outgoing.value.username.trim() === "") {
      return t("setup.validation.outgoingUsername");
    }
    return isOAuth() ? validateOAuth() : validatePasswords();
  }

  function validatePasswords(): string | null {
    if (incoming.value.password === "") return t("setup.validation.incomingPassword");
    if (outgoing.value.password === "") return t("setup.validation.outgoingPassword");
    return null;
  }

  function validateOAuth(): string | null {
    if (clientId.value.trim() === "") return t("setup.validation.clientId");
    if (isCustom()) {
      if (customAuthUrl.value.trim() === "") return t("setup.validation.authUrl");
      if (customTokenUrl.value.trim() === "") return t("setup.validation.tokenUrl");
      if (customScopes.value.trim() === "") return t("setup.validation.scopes");
    }
    return null;
  }

  function buildAuth(): AuthInput {
    if (!isOAuth()) return { mechanism: "password" };
    const oauth: AuthInput["oauth"] = {
      provider: provider.value,
      clientId: clientId.value.trim(),
    };
    if (isCustom()) {
      oauth.authUrl = customAuthUrl.value.trim();
      oauth.tokenUrl = customTokenUrl.value.trim();
      oauth.scopes = customScopes.value.split(/\s+/).filter((s) => s.length > 0);
    }
    return { mechanism: "oauth2", oauth };
  }

  function buildInput(): CreateAccountInput {
    return {
      displayName: displayName.value.trim(),
      email: email.value.trim(),
      auth: buildAuth(),
      incoming: {
        protocol: incoming.value.protocol ?? "imap",
        host: incoming.value.host.trim(),
        port: incoming.value.port,
        security: incoming.value.security,
        username: incoming.value.username.trim(),
        password: isOAuth() ? "" : incoming.value.password,
      },
      outgoing: {
        host: outgoing.value.host.trim(),
        port: outgoing.value.port,
        security: outgoing.value.security,
        username: outgoing.value.username.trim(),
        password: isOAuth() ? "" : outgoing.value.password,
      },
    };
  }

  function messageFrom(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return t("setup.errors.createFailed");
  }

  async function onTest(): Promise<void> {
    if (testing.value) return;
    testResult.value = null;
    testMessage.value = "";
    if (incoming.value.host.trim() === "" || outgoing.value.host.trim() === "") {
      testMessage.value = t("setup.validation.serversBeforeTest");
      return;
    }
    if (isOAuth() && createdAccountId.value === null) {
      testMessage.value = t("setup.validation.signInBeforeTest");
      return;
    }
    testing.value = true;
    try {
      const result =
        isOAuth() && createdAccountId.value !== null
          ? await testAccountConnection(createdAccountId.value)
          : await testConnection(buildInput());
      if (result === null) {
        testMessage.value = t("setup.preview");
        return;
      }
      testResult.value = result;
    } finally {
      testing.value = false;
    }
  }

  async function onSubmit(): Promise<void> {
    if (pending.value) return;
    const problem = validate();
    if (problem !== null) {
      errorMessage.value = problem;
      return;
    }
    errorMessage.value = "";
    pending.value = true;
    try {
      await submitFlow();
    } catch (error) {
      errorMessage.value = messageFrom(error);
    } finally {
      pending.value = false;
    }
  }

  /** Creates the account (once) and, for OAuth2, runs the browser sign-in. */
  async function submitFlow(): Promise<void> {
    if (createdAccountId.value === null) {
      const account = await createAccount(buildInput());
      if (account === null) {
        errorMessage.value = t("setup.preview");
        return;
      }
      createdAccountId.value = account.id;
      createdAccount.value = account;
    }
    if (!isOAuth()) {
      succeeded.value = true;
      onCreated(createdAccount.value as AccountSummary);
      return;
    }
    const result = await oauthAuthorize(createdAccountId.value);
    if (result === null) {
      errorMessage.value = t("setup.preview");
      return;
    }
    if (result.status === "authorized") {
      succeeded.value = true;
      onCreated(createdAccount.value as AccountSummary);
    } else if (result.status === "denied") {
      errorMessage.value = t("setup.errors.signInDeclined");
    } else {
      errorMessage.value = t("setup.errors.signInTimedOut");
    }
  }

  return {
    displayName,
    email,
    mechanism,
    provider,
    clientId,
    customAuthUrl,
    customTokenUrl,
    customScopes,
    incoming,
    outgoing,
    pending,
    errorMessage,
    testing,
    testResult,
    testMessage,
    autoconfigStatus,
    autoconfigSource,
    isOAuth,
    onFindSettings,
    onTest,
    onSubmit,
  };
}
