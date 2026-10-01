// Vite configuration for the Quota renderer.
//
// The server block is the Tauri template's, unchanged: fixed port 1420, strict
// port so a conflict fails loudly, and HMR on 1421 when a dev host is set. The
// React Compiler runs through its documented plugin-react 6.x route, the
// `compiler` option, which uses the Rust port `oxc-transform-react` (spec
// 7.8.4). The previous inline `react({ babel })` form was removed in
// plugin-react 6.0.0, and the Babel preset route was measured and rejected
// because @rolldown/plugin-babel 0.2.4 ships declarations that contradict both
// supported @babel/core lines under `skipLibCheck: false`.
import react from "@vitejs/plugin-react";
import process from "node:process";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(() => ({
  plugins: [react({ compiler: { logDiagnostics: true } })],
  build: {
    // Vite transpiles; `bun run typecheck` is the separate type gate (spec 7.8.1).
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: true,
  },
  // prevent Vite from obscuring rust errors
  clearScreen: false,
  // tauri expects a fixed port, fail if that port is not available
  server: {
    strictPort: true,
    port: 1420,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : false,
    watch: {
      // tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
