import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [vue()],

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
  },
});
