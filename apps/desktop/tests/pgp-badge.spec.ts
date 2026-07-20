import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { analyzeMessagePgp } from "../src/api/pgp";
import PgpBadge from "../src/components/message/PgpBadge.vue";
import type { PgpMessageStatusDto } from "../src/types/pgp";

vi.mock("../src/api/pgp", () => ({ analyzeMessagePgp: vi.fn() }));
const analyzeMock = vi.mocked(analyzeMessagePgp);

function base(over: Partial<PgpMessageStatusDto>): PgpMessageStatusDto {
  return {
    isPgp: false,
    encrypted: false,
    signed: false,
    signatureStatus: "none",
    signer: null,
    decrypted: false,
    needsPassphrase: false,
    bodyHtml: null,
    bodyText: null,
    error: null,
    ...over,
  };
}

describe("PgpBadge", () => {
  beforeEach(() => analyzeMock.mockReset());

  it("renders nothing for a non-PGP message", async () => {
    analyzeMock.mockResolvedValue(base({ isPgp: false }));
    const wrapper = mount(PgpBadge, { props: { messageId: 1 } });
    await flushPromises();
    expect(wrapper.find(".pgp-badge").exists()).toBe(false);
  });

  it("shows a valid-signature badge", async () => {
    analyzeMock.mockResolvedValue(
      base({ isPgp: true, signed: true, signatureStatus: "valid" }),
    );
    const wrapper = mount(PgpBadge, { props: { messageId: 2 } });
    await flushPromises();
    expect(wrapper.find(".pgp-badge").exists()).toBe(true);
    expect(wrapper.text()).toContain("Signature valid");
  });

  it("marks a tampered signature invalid", async () => {
    analyzeMock.mockResolvedValue(
      base({ isPgp: true, signed: true, signatureStatus: "invalid" }),
    );
    const wrapper = mount(PgpBadge, { props: { messageId: 3 } });
    await flushPromises();
    expect(wrapper.text()).toContain("Signature invalid");
    expect(wrapper.find(".chip.bad").exists()).toBe(true);
  });

  it("prompts for a passphrase then decrypts on request", async () => {
    analyzeMock.mockResolvedValueOnce(
      base({ isPgp: true, encrypted: true, needsPassphrase: true }),
    );
    const wrapper = mount(PgpBadge, { props: { messageId: 4 } });
    await flushPromises();
    expect(wrapper.text()).toContain("Enter your key passphrase");

    analyzeMock.mockResolvedValueOnce(
      base({
        isPgp: true,
        encrypted: true,
        decrypted: true,
        bodyText: "the secret plan",
      }),
    );
    await wrapper.find("input[type=password]").setValue("pw");
    await wrapper.find("button").trigger("click");
    await flushPromises();
    expect(analyzeMock).toHaveBeenLastCalledWith(4, "pw");
    expect(wrapper.text()).toContain("the secret plan");
    expect(wrapper.text()).toContain("Decrypted");
  });
});
