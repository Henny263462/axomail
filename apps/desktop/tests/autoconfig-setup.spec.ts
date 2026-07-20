import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { discoverAutoconfig } from "../src/api/autoconfig";
import AccountSetupDialog from "../src/components/setup/AccountSetupDialog.vue";
import type { AutoconfigResult } from "../src/types/mail";

vi.mock("../src/api/autoconfig", () => ({
  discoverAutoconfig: vi.fn(),
}));

const discoverMock = vi.mocked(discoverAutoconfig);

const IMAP_FOUND: AutoconfigResult = {
  found: true,
  source: "ispdb",
  oauth: false,
  displayName: "Example Mail",
  incoming: {
    protocol: "imap",
    host: "imap.example.com",
    port: 993,
    security: "tls",
    username: "klaus@example.com",
  },
  outgoing: {
    host: "smtp.example.com",
    port: 465,
    security: "tls",
    username: "klaus@example.com",
  },
};

const NOT_FOUND: AutoconfigResult = {
  found: false,
  source: null,
  oauth: false,
  displayName: null,
  incoming: null,
  outgoing: null,
};

const value = (wrapper: VueWrapper, id: string): string =>
  (wrapper.get(id).element as HTMLInputElement).value;

beforeEach(() => {
  vi.clearAllMocks();
});

describe("AccountSetupDialog auto-configuration", () => {
  it("autofills the server fields from a found configuration", async () => {
    discoverMock.mockResolvedValue(IMAP_FOUND);
    const wrapper = mount(AccountSetupDialog);
    await wrapper.get("#account-email").setValue("klaus@example.com");

    await wrapper.get(".autoconfig .find").trigger("click");
    await flushPromises();

    expect(discoverMock).toHaveBeenCalledWith("klaus@example.com");
    expect(value(wrapper, "#incoming-host")).toBe("imap.example.com");
    expect(value(wrapper, "#incoming-port")).toBe("993");
    expect(value(wrapper, "#incoming-username")).toBe("klaus@example.com");
    expect(value(wrapper, "#outgoing-host")).toBe("smtp.example.com");
    expect(wrapper.get(".autoconfig .status").text()).toContain(
      "Mozilla provider database",
    );
  });

  it("switches to OAuth2 for a preset provider host", async () => {
    discoverMock.mockResolvedValue({
      ...IMAP_FOUND,
      oauth: true,
      incoming: { ...IMAP_FOUND.incoming!, host: "imap.gmail.com" },
    });
    const wrapper = mount(AccountSetupDialog);
    await wrapper.get("#account-email").setValue("klaus@gmail.com");

    await wrapper.get(".autoconfig .find").trigger("click");
    await flushPromises();

    expect(value(wrapper, "#auth-mechanism")).toBe("oauth2");
    // OAuth2 mode hides the password inputs (browser sign-in instead).
    expect(wrapper.find("#incoming-password").exists()).toBe(false);
  });

  it("reports not-found and leaves the fields empty for manual entry", async () => {
    discoverMock.mockResolvedValue(NOT_FOUND);
    const wrapper = mount(AccountSetupDialog);
    await wrapper.get("#account-email").setValue("someone@obscure.example");

    await wrapper.get(".autoconfig .find").trigger("click");
    await flushPromises();

    expect(wrapper.get(".autoconfig .status").text()).toContain(
      "No settings found automatically",
    );
    // The user can still type settings by hand.
    expect(value(wrapper, "#incoming-host")).toBe("");
  });

  it("nudges for an email before searching", async () => {
    const wrapper = mount(AccountSetupDialog);
    await wrapper.get(".autoconfig .find").trigger("click");
    await flushPromises();

    expect(discoverMock).not.toHaveBeenCalled();
    expect(wrapper.get(".autoconfig .status").text()).toContain(
      "Enter an email address first",
    );
  });

  it("explains that automatic search needs the desktop app in preview", async () => {
    discoverMock.mockResolvedValue(null);
    const wrapper = mount(AccountSetupDialog);
    await wrapper.get("#account-email").setValue("klaus@example.com");

    await wrapper.get(".autoconfig .find").trigger("click");
    await flushPromises();

    expect(wrapper.get(".autoconfig .status").text()).toContain(
      "needs the desktop app",
    );
  });
});
