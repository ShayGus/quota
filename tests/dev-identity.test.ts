// The development identity, proven at the configuration level.
//
// A development build runs beside the production one on one machine, so the
// two must never share an identifier: the identifier decides the data
// directory, the window-state file, the credential service name, the autostart
// entry, and the single-instance lock. These tests read the two configurations
// the build actually merges and check the result.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

interface Window {
  label: string;
  title: string;
  [key: string]: unknown;
}

interface Config {
  productName: string;
  identifier: string;
  mainBinaryName?: string;
  app: {
    windows: Window[];
    [key: string]: unknown;
  };
  bundle?: Record<string, unknown>;
  [key: string]: unknown;
}

function readConfig(name: string): Config {
  return JSON.parse(
    readFileSync(join(import.meta.dirname, "..", "src-tauri", name), "utf8"),
  ) as Config;
}

const production = readConfig("tauri.conf.json");
const overlay = readConfig("tauri.dev.conf.json");

/** What the Tauri CLI merges: objects deep, arrays replaced. */
function development(): Config {
  return {
    ...production,
    ...overlay,
    app: { ...production.app, ...overlay.app },
  };
}

test("the production identity is the one that was always shipped", () => {
  expect(production.identifier).toBe("app.quota.monitor");
  expect(production.productName).toBe("Quota");
  expect(production.mainBinaryName).toBeUndefined();
  expect(production.app.windows.map((window) => window.title)).toEqual([
    "Quota",
    "Quota settings",
    "Quota widget",
  ]);
  expect(production.bundle).toEqual({
    active: true,
    targets: "all",
    icon: [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico",
    ],
  });
});

test("a development package has its own identity and executable name", () => {
  const dev = development();
  expect(dev.identifier).not.toBe(production.identifier);
  expect(dev.productName).not.toBe(production.productName);
  expect(dev.mainBinaryName).toBe("quota-dev");
  expect(dev.mainBinaryName).not.toBe(production.mainBinaryName);
});

test("the development overlay covers every production window", () => {
  const labels = development().app.windows.map((window) => window.label);
  expect(labels).toEqual(production.app.windows.map((window) => window.label));
});

test("every development window says which build it is", () => {
  const productionTitles = production.app.windows.map((window) => window.title);
  const devTitles = development().app.windows.map((window) => window.title);
  expect(devTitles).toEqual(["Quota Dev", "Quota Dev settings", "Quota Dev widget"]);
  expect(devTitles).not.toEqual(productionTitles);
});
