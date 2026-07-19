import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import ComposeBody from "../src/components/compose/ComposeBody.vue";

describe("ComposeBody", () => {
  it("emits sanitized html and a degraded text alternative", async () => {
    const wrapper = mount(ComposeBody);
    await flushPromises();

    const editor = wrapper.get("#compose-editor");
    (editor.element as HTMLElement).innerHTML =
      "<p>Hi <b>there</b></p><script>bad()</script>";
    await editor.trigger("input");

    const html = wrapper.emitted("update:html")?.at(-1)?.[0] as string;
    const text = wrapper.emitted("update:text")?.at(-1)?.[0] as string;
    expect(html).toContain("<b>there</b>");
    expect(html).not.toContain("script");
    expect(text).toContain("Hi there");
  });

  it("degrades to plain text when toggled, dropping the html body", async () => {
    const wrapper = mount(ComposeBody);
    await flushPromises();

    const editor = wrapper.get("#compose-editor");
    (editor.element as HTMLElement).innerHTML = "<p>Line one</p><p>Line two</p>";
    await editor.trigger("input");

    await wrapper.get(".mode-toggle").trigger("click");

    // The plain textarea now holds the degraded text; the html body is null.
    const textarea = wrapper.get("#compose-body").element as HTMLTextAreaElement;
    expect(textarea.value).toContain("Line one");
    expect(textarea.value).toContain("Line two");
    expect(wrapper.emitted("update:html")?.at(-1)?.[0]).toBeNull();
  });

  it("opens a draft in plain-text mode", async () => {
    const wrapper = mount(ComposeBody, {
      props: { isDraft: true, text: "draft body" },
    });
    await flushPromises();

    expect(wrapper.find("#compose-editor").exists()).toBe(false);
    expect(
      (wrapper.get("#compose-body").element as HTMLTextAreaElement).value,
    ).toBe("draft body");
  });

  it("shows the signature preview without baking it into the body", async () => {
    const wrapper = mount(ComposeBody, {
      props: { signatureText: "Klaus\nBerlin" },
    });
    await flushPromises();

    expect(wrapper.get(".signature-body").text()).toContain("Klaus");
    // The editable surface stays empty — the signature is appended on send.
    expect((wrapper.get("#compose-editor").element as HTMLElement).textContent).toBe(
      "",
    );
  });
});
