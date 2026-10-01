// Vite configuration for the Quota renderer.
//
// The React Compiler runs through its documented plugin-react 6.x route: the
// `compiler` option, which uses the Rust port `oxc-transform-react` (spec
// 7.8.4). The previous inline `react({ babel })` form was removed in
// plugin-react 6.0.0, and the Babel preset route was measured and rejected
// because @rolldown/plugin-babel 0.2.4 ships declarations that contradict both
// supported @babel/core lines under `skipLibCheck: false`.
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react({ compiler: { logDiagnostics: true } })],
  build: {
    // Vite transpiles; `pnpm typecheck` is the separate type gate (spec 7.8.1).
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: true,
  },
  clearScreen: false,
  server: {
    strictPort: true,
    port: 5273,
  },
});
