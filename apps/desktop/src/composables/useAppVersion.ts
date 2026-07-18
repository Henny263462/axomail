import { onMounted, ref } from "vue";

import { safeInvoke } from "../lib/tauri";

/**
 * Application version from the Rust side; `null` outside the desktop
 * shell (browser preview, tests).
 */
export function useAppVersion() {
  const version = ref<string | null>(null);

  onMounted(async () => {
    version.value = await safeInvoke<string>("app_version");
  });

  return { version };
}
