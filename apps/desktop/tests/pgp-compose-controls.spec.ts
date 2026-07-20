import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { composeStatus } from "../src/api/pgp";
import ComposePgpControls from "../src/components/compose/ComposePgpControls.vue";
import type { PgpComposeStatusDto } from "../src/types/pgp";

vi.mock("../src/api/pgp", () => ({ composeStatus: vi.fn() }));
const composeStatusMock = vi.mocked(composeStatus);

function status(over: Partial<PgpComposeStatusDto>): PgpComposeStatusDto {
  return { canSign: false, canEncrypt: false, missingRecipients: [], ...over };
}

/** A promise the test resolves later, to model a slow availability lookup. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

describe("ComposePgpControls", () => {
  beforeEach(() => composeStatusMock.mockReset());

  it("enables signing and encryption when keys exist for everyone", async () => {
    composeStatusMock.mockResolvedValue(
      status({ canSign: true, canEncrypt: true }),
    );
    const wrapper = mount(ComposePgpControls, {
      props: {
        fromEmail: "alice@example.org",
        recipients: ["bob@example.org"],
        sign: false,
        encrypt: false,
        passphrase: "",
      },
    });
    await flushPromises();
    const boxes = wrapper.findAll("input[type=checkbox]");
    const signBox = boxes[0];
    const encryptBox = boxes[1];
    if (!signBox || !encryptBox) throw new Error("expected two checkboxes");
    expect((signBox.element as HTMLInputElement).disabled).toBe(false);
    expect((encryptBox.element as HTMLInputElement).disabled).toBe(false);
  });

  it("disables encryption and names missing recipients", async () => {
    composeStatusMock.mockResolvedValue(
      status({ canSign: true, missingRecipients: ["carol@example.org"] }),
    );
    const wrapper = mount(ComposePgpControls, {
      props: {
        fromEmail: "alice@example.org",
        recipients: ["carol@example.org"],
        sign: false,
        encrypt: false,
        passphrase: "",
      },
    });
    await flushPromises();
    const encryptBox = wrapper.findAll("input[type=checkbox]")[1];
    if (!encryptBox) throw new Error("expected the encrypt checkbox");
    expect((encryptBox.element as HTMLInputElement).disabled).toBe(true);
    expect(wrapper.text()).toContain("carol@example.org");
  });

  it("switches off a now-impossible choice", async () => {
    composeStatusMock.mockResolvedValue(status({ canSign: false }));
    const wrapper = mount(ComposePgpControls, {
      props: {
        fromEmail: "alice@example.org",
        recipients: ["bob@example.org"],
        sign: true,
        encrypt: false,
        passphrase: "",
      },
    });
    await flushPromises();
    // Sign was requested but is no longer possible → the control asks to clear it.
    expect(wrapper.emitted("update:sign")?.at(-1)).toEqual([false]);
  });

  // Finding 3: an out-of-order availability response must not re-enable Encrypt
  // when the current recipient set lacks a key.
  it("ignores a stale canEncrypt:true once a keyless recipient was added", async () => {
    const slow = deferred<PgpComposeStatusDto>();
    composeStatusMock
      .mockReturnValueOnce(slow.promise) // [bob]: slow, would allow encryption
      .mockResolvedValueOnce(
        status({ canSign: true, missingRecipients: ["carol@example.org"] }),
      ); // [bob, carol]: fast, cannot encrypt

    const wrapper = mount(ComposePgpControls, {
      props: {
        fromEmail: "alice@example.org",
        recipients: ["bob@example.org"],
        sign: false,
        encrypt: false,
        passphrase: "",
      },
    });
    // A keyless recipient is added before the first lookup resolves.
    await wrapper.setProps({
      recipients: ["bob@example.org", "carol@example.org"],
    });
    await flushPromises();

    const encryptDisabled = (): boolean =>
      (wrapper.findAll("input[type=checkbox]")[1]!.element as HTMLInputElement)
        .disabled;
    // The newer lookup applied: encryption is off.
    expect(encryptDisabled()).toBe(true);

    // The stale lookup for [bob] resolves last, claiming encryption is possible.
    slow.resolve(status({ canSign: true, canEncrypt: true }));
    await flushPromises();
    // It is discarded — Encrypt stays disabled for the current keyless set.
    expect(encryptDisabled()).toBe(true);
  });
});
