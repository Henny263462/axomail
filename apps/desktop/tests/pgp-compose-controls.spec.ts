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
});
