import { createApp } from "vue";

import App from "./App.vue";
import { initTheme } from "./composables/useTheme";
import { i18n, initLanguage } from "./i18n";
import "./styles/base.css";

// Paint the cached theme before the first frame to avoid a light/dark flash;
// App.vue reconciles it with the persisted backend setting once mounted.
initTheme();
// Likewise pick the cached UI language before mount so the first frame renders
// in the user's language; App.vue reconciles it with the backend once mounted.
initLanguage();

createApp(App).use(i18n).mount("#app");
