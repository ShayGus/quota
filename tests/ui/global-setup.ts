/**
 * Builds the faked-host script once before the interface tests run.
 *
 * The script is the test double in `fake-backend.ts` plus Tauri's own mock
 * helpers, bundled into one file that each test injects ahead of the renderer.
 * It is written under `node_modules/.cache`, which Git and the formatters
 * already ignore, and never into `dist`, so it cannot reach a release build.
 */
import { build } from "vite";

import { FAKE_BACKEND_DIRECTORY, FAKE_BACKEND_FILE } from "./paths";

export default async function globalSetup(): Promise<void> {
  await build({
    configFile: false,
    logLevel: "error",
    build: {
      outDir: FAKE_BACKEND_DIRECTORY,
      emptyOutDir: true,
      minify: false,
      lib: {
        entry: new URL("fake-backend.ts", import.meta.url).pathname,
        formats: ["iife"],
        name: "QuotaFakeBackend",
        fileName: () => FAKE_BACKEND_FILE,
      },
    },
  });
}
