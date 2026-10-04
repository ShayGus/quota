// The version the windows report is the one the build is stamped with, not a
// literal: after an update the interface must name the new version.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

import { APP_VERSION } from "../src/shared/version";

function version(file: string): string {
  const text = readFileSync(join(import.meta.dirname, "..", file), "utf8");
  return (JSON.parse(text) as { version: string }).version;
}

test("the reported version is package.json's, which is the Tauri configuration's", () => {
  expect(APP_VERSION).toBe(version("package.json"));
  expect(APP_VERSION).toBe(version("src-tauri/tauri.conf.json"));
});
