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

  it("opens activity from the always-present activity button", async () => {
    const wrapper = mount(StatusBar);
    await wrapper.get('button[aria-label="Activity"]').trigger("click");
    expect(wrapper.emitted("openActivity")).toHaveLength(1);
  });

  it("shows a running indicator with a cancel affordance while syncing", async () => {
    const wrapper = mount(StatusBar, { props: { syncing: true, syncCount: 3 } });
    expect(wrapper.text()).toContain("Syncing 3…");

    await wrapper.get('button[aria-label="Cancel all sync jobs"]').trigger("click");
    expect(wrapper.emitted("cancelSync")).toHaveLength(1);
  });

  it("hides the running indicator when nothing is syncing", () => {
    const wrapper = mount(StatusBar);
    expect(wrapper.text()).not.toContain("Syncing");
    expect(
      wrapper.find('button[aria-label="Cancel all sync jobs"]').exists(),
    ).toBe(false);
  });
});
