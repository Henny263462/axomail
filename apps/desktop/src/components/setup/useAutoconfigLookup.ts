/**
 * The "find settings automatically" step of account setup.
 *
 * Kept separate from {@link useAccountSetup} so the server-discovery concern
 * stays isolated from account creation and OAuth: this owns the lookup status
 * and, on success, writes the discovered host/port/security/username (and, for
 * a shipped OAuth2 provider, the auth method) into the form refs the caller
 * passes in. No secret is involved — the result is a suggestion the user
 * reviews, tests, and may edit before saving.
 */
import { ref, type Ref } from "vue";

import { discoverAutoconfig } from "../../api/autoconfig";
import type {
  AuthMechanism,
  AutoconfigResult,
  AutoconfigSource,
  OAuthProvider,
} from "../../types/mail";
import type { ServerDraft } from "./draft";

/**
 * Lifecycle of the lookup: idle, searching, a result (found via a source /
 * nothing found), the missing-email nudge, or the browser-preview case.
 */
export type AutoconfigStatus =
  | "idle"
  | "needEmail"
  | "searching"
  | "found"
  | "notFound"
  | "unavailable";

/** The form refs the lookup fills in on a successful discovery. */
interface AutoconfigForm {
  email: Ref<string>;
  incoming: Ref<ServerDraft>;
  outgoing: Ref<ServerDraft>;
  mechanism: Ref<AuthMechanism>;
  provider: Ref<OAuthProvider>;
}

/**
 * Maps a discovered incoming host to a shipped OAuth2 preset, or `null` when
 * no preset applies (the caller then keeps password authentication).
 */
function presetProviderFor(host: string): OAuthProvider | null {
  const h = host.toLowerCase();
  if (h.includes("gmail") || h.includes("google")) return "gmail";
  if (h.includes("office365") || h.includes("outlook") || h.includes("live")) {
    return "microsoft";
  }
  return null;
}

export function useAutoconfigLookup(form: AutoconfigForm) {
  const autoconfigStatus = ref<AutoconfigStatus>("idle");
  const autoconfigSource = ref<AutoconfigSource | null>(null);

  // Monotonic guard: a lookup is bound to the email it was started for, so a
  // response that resolves after the user edits the address never overwrites
  // the newer form.
  let lookupToken = 0;

  /**
   * Runs auto-configuration for the typed email and, on success, fills the
   * server fields (and the auth method for OAuth2 providers). Every branch
   * updates {@link autoconfigStatus} so the panel reports progress honestly.
   */
  async function onFindSettings(): Promise<void> {
    const trimmed = form.email.value.trim();
    if (trimmed === "" || !trimmed.includes("@")) {
      autoconfigStatus.value = "needEmail";
      return;
    }
    if (autoconfigStatus.value === "searching") return;
    const token = ++lookupToken;
    autoconfigStatus.value = "searching";
    autoconfigSource.value = null;
    const result = await discoverAutoconfig(trimmed);
    // The address was edited (or another lookup began) while this was in
    // flight: abandon the stale result rather than write it into the new form,
    // and return to idle so a fresh search can run for the current address.
    if (token !== lookupToken || form.email.value.trim() !== trimmed) {
      if (token === lookupToken) autoconfigStatus.value = "idle";
      return;
    }
    if (result === null) {
      autoconfigStatus.value = "unavailable";
      return;
    }
    if (!result.found) {
      autoconfigStatus.value = "notFound";
      return;
    }
    applyAutoconfig(result);
    autoconfigSource.value = result.source;
    autoconfigStatus.value = "found";
  }

  /** Writes a discovered configuration into the server (and auth) fields. */
  function applyAutoconfig(result: AutoconfigResult): void {
    if (result.incoming !== null) {
      form.incoming.value = {
        protocol: result.incoming.protocol,
        host: result.incoming.host,
        port: result.incoming.port,
        security: result.incoming.security,
        username: result.incoming.username,
        password: form.incoming.value.password,
      };
    }
    if (result.outgoing !== null) {
      form.outgoing.value = {
        host: result.outgoing.host,
        port: result.outgoing.port,
        security: result.outgoing.security,
        username: result.outgoing.username,
        password: form.outgoing.value.password,
      };
    }
    // Switch to OAuth2 only when the host maps to a preset we ship; a custom
    // OAuth provider needs endpoints autoconfig does not supply, so leaving it
    // on password (app-password) is the safer default there.
    const oauthProvider =
      result.oauth && result.incoming !== null
        ? presetProviderFor(result.incoming.host)
        : null;
    if (oauthProvider !== null) {
      form.mechanism.value = "oauth2";
      form.provider.value = oauthProvider;
    }
  }

  return { autoconfigStatus, autoconfigSource, onFindSettings };
}
