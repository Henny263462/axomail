/**
 * Vitest global setup: makes the shared Vue I18n instance available to every
 * component mounted through `@vue/test-utils`, so a component that uses `$t` or
 * `useI18n({ useScope: "global" })` renders its strings instead of throwing.
 *
 * The locale is reset to English before each test so a test that switches the
 * language (e.g. the language-section spec) cannot leak into the next one.
 */
import { config } from "@vue/test-utils";
import { beforeEach } from "vitest";

import { i18n } from "../src/i18n";

config.global.plugins = [i18n];

beforeEach(() => {
  i18n.global.locale.value = "en";
});
