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
    windows: {
      nsis: {
        installMode: "currentUser",
      },
    },
    macOS: {
      signingIdentity: "-",
    },
    linux: {
      appimage: {
        files: {
          "usr/lib/libayatana-appindicator3.so.1":
            "/usr/lib/x86_64-linux-gnu/libayatana-appindicator3.so.1",
          "usr/lib/libayatana-indicator3.so.7":
            "/usr/lib/x86_64-linux-gnu/libayatana-indicator3.so.7",
          "usr/lib/libayatana-ido3-0.4.so.0":
            "/usr/lib/x86_64-linux-gnu/libayatana-ido3-0.4.so.0",
          "usr/lib/libdbusmenu-gtk3.so.4":
            "/usr/lib/x86_64-linux-gnu/libdbusmenu-gtk3.so.4",
          "usr/lib/libdbusmenu-glib.so.4":
            "/usr/lib/x86_64-linux-gnu/libdbusmenu-glib.so.4",
        },
      },
      deb: {
        depends: ["libwebkit2gtk-4.1-0", "libayatana-appindicator3-1", "libssl3"],
      },
      rpm: {
        depends: ["webkit2gtk4.1", "libappindicator-gtk3", "openssl-libs"],
      },
    },
    createUpdaterArtifacts: true,
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

test("a development package signs nothing and carries no update settings", () => {
  // The release signs every package for the updater; a development package needs
  // no key, and an overlay must not be able to change where updates come from.
  expect(production.bundle?.["createUpdaterArtifacts"]).toBe(true);
  expect(overlay.bundle).toEqual({ createUpdaterArtifacts: false });
  expect(overlay).not.toHaveProperty("plugins");
});
