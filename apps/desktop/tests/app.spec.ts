import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import App from "../src/App.vue";

describe("App", () => {
  it("renders the three-column shell with honest empty states", async () => {
    const wrapper = mount(App);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(wrapper.get('[aria-label="Accounts and folders"]').text()).toContain(
      "No accounts",
    );
    expect(wrapper.get('[aria-label="Message list"]').text()).toContain(
      "No messages",
    );
    expect(wrapper.get('[aria-label="Message"]').text()).toContain(
      "No message selected",
    );
  });

  it("labels the session as a browser preview outside Tauri", () => {
    const wrapper = mount(App);
    const status = wrapper.get('[aria-label="Status"]');
    expect(status.text()).toContain("axomail");
    expect(status.text()).toContain("Browser preview");
  });

  it("does not load fixtures without the dev-fixtures flag", async () => {
    const wrapper = mount(App);
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(wrapper.text()).not.toContain("Fixture account");
  });
});
