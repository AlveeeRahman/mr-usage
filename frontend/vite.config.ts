import { defineConfig } from "vite";

// Tauri expects a fixed dev port and serves the built files itself.
// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  // The window lives in this folder. `npm run` always starts from the repository root, so the
  // path is relative to it; the built files go to <repo>/dist, where src-tauri/tauri.conf.json looks.
  root: "frontend",
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
    outDir: "../dist",
    emptyOutDir: true,
  },
});
