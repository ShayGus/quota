// Vitest configuration for the real-app suite.
//
// These tests start the built desktop application under a display and drive it
// through tauri-driver, so they are slow, need system programs, and must not run
// as part of `bun run test`. They run one at a time: each journey owns a
// sandbox, but they share one display and the machine's patience.
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    globals: false,
    include: ["tests/e2e/**/*.e2e.ts"],
    fileParallelism: false,
    testTimeout: 120_000,
    hookTimeout: 60_000,
    reporters: ["default"],
  },
});
