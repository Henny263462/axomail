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

/** A promise whose resolution the test drives, to model a slow decrypt. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
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

  // Finding 1: a decrypt is tied to the message it was started for.
  it("discards a stale decrypt result for a previously-selected message", async () => {
    const staleDecrypt = deferred<PgpMessageStatusDto>();
    analyzeMock
      .mockResolvedValueOnce(
        base({ isPgp: true, encrypted: true, needsPassphrase: true }),
      ) // message 10, initial analyze
      .mockReturnValueOnce(staleDecrypt.promise) // message 10, slow decrypt
      .mockResolvedValueOnce(base({ isPgp: false })); // message 20, initial analyze

    const wrapper = mount(PgpBadge, { props: { messageId: 10 } });
    await flushPromises();
    await wrapper.find("input[type=password]").setValue("pw");
    await wrapper.find("button").trigger("click"); // decrypt for message 10 in flight

    // Switch to message 20 before message 10's decrypt resolves.
    await wrapper.setProps({ messageId: 20 });
    await flushPromises();

    // The slow decrypt for message 10 now resolves with its plaintext; it must
    // NOT be rendered while message 20 is selected.
    staleDecrypt.resolve(
      base({
        isPgp: true,
        encrypted: true,
        decrypted: true,
        bodyText: "message 10 plaintext",
      }),
    );
    await flushPromises();
    expect(wrapper.text()).not.toContain("message 10 plaintext");
  });

  // Finding 2: the reader passphrase is not retained after the attempt.
  it("clears the passphrase field after a decrypt attempt", async () => {
    analyzeMock
      .mockResolvedValueOnce(
        base({ isPgp: true, encrypted: true, needsPassphrase: true }),
      )
      .mockResolvedValueOnce(
        base({ isPgp: true, encrypted: true, needsPassphrase: true }),
      );
    const wrapper = mount(PgpBadge, { props: { messageId: 4 } });
    await flushPromises();

    await wrapper.find("input[type=password]").setValue("wrong");
    await wrapper.find("button").trigger("click");
    await flushPromises();

    // It was passed to the backend, then wiped from the field (never retained
    // in reactive state).
    expect(analyzeMock).toHaveBeenLastCalledWith(4, "wrong");
    expect(
      (wrapper.find("input[type=password]").element as HTMLInputElement).value,
    ).toBe("");
  });

  // Finding 4: a decrypted HTML-only body renders (sandboxed), not blank.
  it("renders a decrypted HTML-only body through the sandboxed iframe", async () => {
    analyzeMock.mockResolvedValue(
      base({
        isPgp: true,
        encrypted: true,
        decrypted: true,
        bodyText: null,
        bodyHtml: "<p>secret html body</p>",
      }),
    );
    const wrapper = mount(PgpBadge, { props: { messageId: 6 } });
    await flushPromises();

    const frame = wrapper.find("iframe.html-frame");
    expect(frame.exists()).toBe(true);
    // Same ADR-0007 posture as normal mail: null-origin, script-less sandbox.
    expect(frame.attributes("sandbox")).toBe("");
    expect(frame.attributes("srcdoc")).toContain("secret html body");
  });

  // Coordination with the Rust signer DTO: a valid signature names the signer.
  it("names the signer on a valid signature", async () => {
    analyzeMock.mockResolvedValue(
      base({
        isPgp: true,
        signed: true,
        signatureStatus: "valid",
        signer: "Ada <ada@example.org>",
      }),
    );
    const wrapper = mount(PgpBadge, { props: { messageId: 7 } });
    await flushPromises();
    expect(wrapper.text()).toContain("Ada <ada@example.org>");
    expect(wrapper.find(".chip.ok").exists()).toBe(true);
  });

  // A signature valid under a non-sender key is a WARNING, never green "valid".
  it("shows a signer-mismatch signature as a warning, not a green valid chip", async () => {
    analyzeMock.mockResolvedValue(
      base({
        isPgp: true,
        signed: true,
        signatureStatus: "signerMismatch",
        signer: "Eve <eve@example.org>",
      }),
    );
    const wrapper = mount(PgpBadge, { props: { messageId: 8 } });
    await flushPromises();

    const sigChip = wrapper
      .findAll(".chip")
      .find((chip) => chip.text().includes("not by the sender"));
    expect(sigChip).toBeTruthy();
    expect(sigChip?.classes()).toContain("warn");
    expect(sigChip?.classes()).not.toContain("ok");
  });
});
