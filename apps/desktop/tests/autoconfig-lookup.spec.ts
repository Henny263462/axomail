import { beforeEach, describe, expect, it, vi } from "vitest";
import { ref } from "vue";

import { discoverAutoconfig } from "../src/api/autoconfig";
import { useAutoconfigLookup } from "../src/components/setup/useAutoconfigLookup";
import type { ServerDraft } from "../src/components/setup/draft";
import type {
  AuthMechanism,
  AutoconfigResult,
  OAuthProvider,
} from "../src/types/mail";

vi.mock("../src/api/autoconfig", () => ({ discoverAutoconfig: vi.fn() }));
const discoverMock = vi.mocked(discoverAutoconfig);

/** A promise the test resolves later, to model a slow autoconfig lookup. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

function emptyIncoming(): ServerDraft {
  return {
    protocol: "imap",
    host: "",
    port: 0,
    security: "tls",
    username: "",
    password: "",
  };
}

function makeForm() {
  return {
    email: ref(""),
    incoming: ref<ServerDraft>(emptyIncoming()),
    outgoing: ref<ServerDraft>({
      host: "",
      port: 0,
      security: "tls",
      username: "",
      password: "",
    }),
    mechanism: ref<AuthMechanism>("password"),
    provider: ref<OAuthProvider>("gmail"),
  };
}

const foundFor = (host: string): AutoconfigResult => ({
  found: true,
  source: "ispdb",
  oauth: false,
  displayName: "Example",
  incoming: {
    protocol: "imap",
    host,
    port: 993,
    security: "tls",
    username: "u@example.com",
  },
  outgoing: {
    host: `smtp.${host}`,
    port: 465,
    security: "tls",
    username: "u@example.com",
  },
});

beforeEach(() => {
  discoverMock.mockReset();
});

describe("useAutoconfigLookup", () => {
  it("fills the form from a found configuration", async () => {
    discoverMock.mockResolvedValue(foundFor("imap.first.example"));
    const form = makeForm();
    form.email.value = "a@first.example";
    const { onFindSettings, autoconfigStatus } = useAutoconfigLookup(form);

    await onFindSettings();

    expect(form.incoming.value.host).toBe("imap.first.example");
    expect(autoconfigStatus.value).toBe("found");
  });

  // Finding 8: a response is bound to the email it was requested for, so
  // editing the address mid-lookup cannot let a stale result overwrite the form.
  it("discards a response whose email no longer matches the form", async () => {
    const slow = deferred<AutoconfigResult>();
    discoverMock.mockReturnValueOnce(slow.promise);

    const form = makeForm();
    form.email.value = "a@first.example";
    const { onFindSettings, autoconfigStatus } = useAutoconfigLookup(form);

    const pending = onFindSettings();
    // The user edits the address while the first lookup is in flight.
    form.email.value = "b@second.example";
    // The stale lookup (for the first address) resolves last.
    slow.resolve(foundFor("imap.first.example"));
    await pending;

    // The first address's config was NOT written into the newer form.
    expect(form.incoming.value.host).toBe("");
    expect(autoconfigStatus.value).not.toBe("found");
  });
});
