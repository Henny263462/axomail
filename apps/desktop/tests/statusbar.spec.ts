import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import StatusBar from "../src/components/layout/StatusBar.vue";

describe("StatusBar", () => {
  it("shows the browser-preview label without an error", () => {
    const wrapper = mount(StatusBar);
    expect(wrapper.text()).toContain("axomail");
    expect(wrapper.text()).toContain("Browser preview");
  });

  it("surfaces a job failure and emits dismiss", async () => {
    const wrapper = mount(StatusBar, {
      props: { error: "Sync failed: server unreachable." },
    });
    const alert = wrapper.get('[role="alert"]');
    expect(alert.text()).toContain("Sync failed: server unreachable.");
    // The preview label yields to the error while one is present.
    expect(wrapper.text()).not.toContain("Browser preview");

    await wrapper.get("button.dismiss").trigger("click");
    expect(wrapper.emitted("dismissError")).toHaveLength(1);
  });
});
