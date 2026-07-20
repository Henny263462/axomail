import { createApp } from "vue";

import App from "./App.vue";
import { initLayout } from "./composables/useLayout";
import { initPaneSizes } from "./composables/usePaneSizes";
import { initTheme } from "./composables/useTheme";
import { i18n, initLanguage } from "./i18n";
import "./styles/base.css";

// Paint the cached theme before the first frame to avoid a light/dark flash;
// App.vue reconciles it with the persisted backend setting once mounted.
initTheme();
// Likewise pick the cached UI language before mount so the first frame renders
// in the user's language; App.vue reconciles it with the backend once mounted.
initLanguage();
// Restore the persisted pane layout and splitter sizes before mount so the
// shell paints in the user's chosen arrangement from the first frame. Both are
// pure UI preferences (localStorage), so there is no backend reconcile.
initLayout();
initPaneSizes();

createApp(App).use(i18n).mount("#app");
