/** Vite configuration for same-origin production and proxied local development. */

import tailwindcss from "@tailwindcss/vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "node:path";
import { defineConfig } from "vite";

/** Local development server target for API and health requests. */
const SERVER_TARGET = "http://127.0.0.1:3000";

/** Builds the Vue application without remote runtime dependencies. */
export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: { "@": resolve(__dirname, "src") },
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": SERVER_TARGET,
      "/health": SERVER_TARGET,
    },
  },
});
