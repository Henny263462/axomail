import js from "@eslint/js";
import pluginVue from "eslint-plugin-vue";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist/**", "src-tauri/**"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...pluginVue.configs["flat/recommended"],
  {
    files: ["**/*.vue"],
    languageOptions: {
      parserOptions: { parser: tseslint.parser },
    },
    rules: {
      // `<script setup lang="ts">` blocks are type-checked by vue-tsc, which
      // already reports undefined identifiers and understands DOM lib types
      // (Event, HTMLElement, document, …). The base `no-undef` rule cannot
      // resolve those TypeScript type references, so — exactly as
      // typescript-eslint does for `.ts` files — it is disabled here.
      "no-undef": "off",
    },
  },
  {
    rules: {
      // Hard ceiling from the project conventions; the 300-line guideline
      // is documented in CONTRIBUTING (work package 1d).
      "max-lines": ["error", { max: 400, skipBlankLines: true, skipComments: true }],
    },
  },
);
