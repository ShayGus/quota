// Playwright configuration for the interface tests.
//
// The tests load the real built renderer (`bun run build`, served by
// `vite preview`) in headless Chromium, with the Tauri backend faked at the IPC
// boundary. No native shell, no Rust, and no network beyond localhost is
// involved, so the suite is the same on a laptop and on a CI runner.
import { defineConfig } from "@playwright/test";
import process from "node:process";

const PORT = 4173;
const ci = process.env["CI"] !== undefined;

export default defineConfig({
  testDir: "tests/ui",
  testMatch: "**/*.spec.ts",
  globalSetup: "./tests/ui/global-setup.ts",
  globalTeardown: "./tests/ui/global-teardown.ts",
  fullyParallel: true,
  forbidOnly: ci,
  // A retry would hide a flaky interface test; a failure is a failure.
  retries: 0,
  reporter: ci
    ? [["list"], ["html", { open: "never", outputFolder: "playwright-report" }]]
    : [["list"]],
  outputDir: "test-results/ui",
  use: {
    baseURL: `http://127.0.0.1:${String(PORT)}`,
    browserName: "chromium",
    deviceScaleFactor: 1,
    colorScheme: "light",
    trace: "retain-on-failure",
  },
  webServer: {
    command: `bun run build && bun run preview --host 127.0.0.1 --port ${String(PORT)} --strictPort`,
    url: `http://127.0.0.1:${String(PORT)}/index.html`,
    reuseExistingServer: !ci,
    timeout: 120_000,
  },
});
