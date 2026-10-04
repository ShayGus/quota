// Vitest configuration for the renderer test project.
//
// jsdom supplies the DOM; the environment is stated per file through the
// `// @vitest-environment` comment only when a file differs from this default.
import react from "@vitejs/plugin-react";
import { readFileSync } from "node:fs";
import { defineConfig } from "vitest/config";

const version = (JSON.parse(readFileSync("package.json", "utf8")) as { version: string })
  .version;

export default defineConfig({
  plugins: [react({ compiler: { logDiagnostics: true } })],
  define: { __QUOTA_VERSION__: JSON.stringify(version) },
  test: {
    environment: "jsdom",
    globals: false,
    include: ["tests/**/*.test.{ts,tsx}"],
    setupFiles: ["tests/setup.ts"],
    restoreMocks: true,
    mockReset: true,
    unstubEnvs: true,
    unstubGlobals: true,
  },
});
