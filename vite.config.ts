import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

const host = process.env.TAURI_DEV_HOST;

/**
 * Tauri 2's custom asset scheme on Windows (`http://tauri.localhost/`) serves
 * the bundled JS without CORS headers. Vite's default emits
 * `<script type="module" crossorigin src="…">`, and WebView2 then refuses
 * to execute the module because it interprets the `crossorigin` attribute as
 * "require CORS-allowed response" — same-origin request, but the strict mode
 * still blocks it. The script never executes, the page stays blank, no
 * console error in some configurations.
 *
 * Strip the attribute. Same-origin requests don't need it anyway in our case.
 */
const stripCrossOriginPlugin = {
  name: "gsyncing-strip-crossorigin",
  transformIndexHtml(html: string) {
    return html.replace(/\s+crossorigin(?=[ >])/g, "");
  },
};

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), stripCrossOriginPlugin],
  // Use relative asset paths so the production HTML resolves chunks under
  // `tauri://localhost/...` no matter what mount root the webview picks.
  // Default `/` works in dev (vite serves from http root) but in some Tauri 2
  // webview configurations the absolute path resolves wrong → white screen.
  base: "./",
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  // Vite options tailored for Tauri development
  clearScreen: false,
  build: {
    // ABANDONED manualChunks (was tried in v0.8 and again in v1.4.0/1):
    // Splitting node_modules into vendor-antd / vendor-react / vendor-other
    // chunks causes a `Cannot access 'ms' before initialization` TDZ error
    // at runtime when Tauri 2's WebView2 loads the chunks. Some transitive
    // dep (likely `ms` consumed via `dayjs` / `debug`) sits across the chunk
    // boundary in a way that breaks ESM init order. Reproduced by user
    // 2026-05-22 in the v1.4.0 release.
    //
    // For a local desktop app a single ~1.16 MB bundle loads in milliseconds
    // off disk anyway. Not worth fighting bundler heuristics for kilobytes.
    chunkSizeWarningLimit: 2000,
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
