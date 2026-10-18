import { defineConfig } from "vite";

/**
 * Little Log frontend build.
 *
 * - `npm run dev`   → dev server with HMR; /api is proxied to the Rust server
 *                     (little-log-server, default port 8080)
 * - `npm run build` → `dist/`, which the axum server embeds/serves and Tauri
 *                     uses as `frontendDist`
 */
export default defineConfig({
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: true,
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": { target: "http://localhost:8080", changeOrigin: false },
    },
  },
  clearScreen: false,
});
