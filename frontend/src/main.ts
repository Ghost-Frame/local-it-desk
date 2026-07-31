/** Browser entry point for the local help desk. */

import { createPinia } from "pinia";
import { createApp } from "vue";

import App from "./App.vue";
import "./assets/main.css";
import { router } from "./router";

/** Root Vue application instance. */
const app = createApp(App);
app.use(createPinia());
app.use(router);
app.mount("#app");
