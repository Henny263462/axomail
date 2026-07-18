import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import App from "../src/App.vue";

describe("App", () => {
  it("mounts and renders the shell outside of a Tauri context", async () => {
    const wrapper = mount(App);

    expect(wrapper.get("h1").text()).toBe("axomail");

    // The `app_version` invoke rejects outside Tauri; the component must
    // swallow that and keep the version line hidden instead of crashing.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(wrapper.find(".version").exists()).toBe(false);
  });
});
