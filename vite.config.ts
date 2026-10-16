import { defineConfig } from "vite";

// Tauri expects a fixed dev port and serves the built files itself.
// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "localhost",
  },
  build: {
    target: "es2022",
    sourcemap: false,
    modulePreload: { polyfill: false },
    outDir: "dist",
    emptyOutDir: true,
  },
});
