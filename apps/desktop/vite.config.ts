// Vite configuration for the Quota renderer.
//
// The React Compiler is enabled through its documented React 19 route: the
// `reactCompilerPreset` helper from `@vitejs/plugin-react` 6.x driven by
// `@rolldown/plugin-babel` (spec 7.8.4). The previous inline `react({ babel })`
// form was removed in plugin-react 6.0.0, so it is not used here.
import babel from "@rolldown/plugin-babel";
import react, { reactCompilerPreset } from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), babel({ presets: [reactCompilerPreset()] })],
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
