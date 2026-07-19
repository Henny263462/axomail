import { createApp } from "vue";

import App from "./App.vue";
import { initTheme } from "./composables/useTheme";
import "./styles/base.css";

// Paint the cached theme before the first frame to avoid a light/dark flash;
// App.vue reconciles it with the persisted backend setting once mounted.
initTheme();

createApp(App).mount("#app");
