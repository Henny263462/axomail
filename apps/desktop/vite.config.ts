import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [vue()],

  // Vue I18n build-time feature flags. Defining them explicitly silences the
  // "feature flag not explicitly defined" advisory and documents the choices:
  // composition API only (no legacy), no production devtools, and — crucially —
  // the message compiler is KEPT (`__INTLIFY_DROP_MESSAGE_COMPILER__: false`)
  // so the plain JSON locale catalogues compile just-in-time at runtime, which
  // is why no message-precompiler plugin is needed.
  define: {
    __VUE_I18N_FULL_INSTALL__: true,
    __VUE_I18N_LEGACY_API__: false,
    __INTLIFY_PROD_DEVTOOLS__: false,
    __INTLIFY_DROP_MESSAGE_COMPILER__: false,
  },

  // Vite options tailored for Tauri development:
  // keep Rust compiler errors visible and use the fixed port Tauri expects.
  // Desktop-only: no TAURI_DEV_HOST remote-device handling; the dev CSP in
  // tauri.conf.json only allows ws://localhost:1420.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },

  test: {
    environment: "happy-dom",
    include: ["tests/**/*.spec.ts"],
    // Installs the shared i18n instance on every mounted component (via
    // @vue/test-utils global config) and resets the locale between tests.
    setupFiles: ["tests/setup.ts"],
  },
});
