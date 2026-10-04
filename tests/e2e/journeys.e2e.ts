/**
 * The real-app journeys: the built Quota application, started under a display
 * and driven through tauri-driver, with a throwaway HOME for every journey.
 *
 * Two debug binaries are used. `QUOTA_E2E_APP` is the ordinary build, which
 * starts with no accounts. `QUOTA_E2E_SAMPLE_APP` is the same build with the
 * `sample-data` feature, which seeds ten deterministic accounts so the account
 * screens have something to show; it keeps its data under a `sample` folder.
 * `bun run build:e2e` builds both.
 */
import { existsSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";

import { afterEach, describe, expect, it } from "vitest";

import { findWindow, Sandbox, waitUntilVisible, type RunningApp } from "./sandbox";
import { startFakeProvider } from "./fake-provider";
import { waitFor } from "./webdriver";

const EXE = process.platform === "win32" ? ".exe" : "";
const APP =
  process.env["QUOTA_E2E_APP"] ?? join("src-tauri", "target", "e2e", `quota${EXE}`);
const SAMPLE_APP =
  process.env["QUOTA_E2E_SAMPLE_APP"] ??
  join("src-tauri", "target", "e2e", `quota-sample${EXE}`);

/** The identity a debug build must run under, from `tauri.dev.conf.json`. */
const DEV_IDENTIFIER = "app.quota.monitor.dev";
const PRODUCTION_IDENTIFIER = "app.quota.monitor";

let sandbox: Sandbox | null = null;

afterEach(async () => {
  await sandbox?.destroy();
  sandbox = null;
});

/** A fresh sandbox for the journey that is running. */
async function begin(name: string): Promise<Sandbox> {
  sandbox = await Sandbox.create(name);
  return sandbox;
}

/** Starts the overview and waits until it is on screen. */
async function startOverview(world: Sandbox, binary: string): Promise<RunningApp> {
  const app = await world.launch(binary);
  await app.session.switchTo(await findWindow(app.session, "overview"));
  await waitUntilVisible(app.session, "the overview");
  return app;
}

/** Exits from the tray on Linux, or ends the WebDriver session on Windows. */
async function quit(world: Sandbox, app: RunningApp): Promise<void> {
  if (process.platform === "win32") {
    await app.stop();
    return;
  }
  expect(await waitFor("the tray menu", () => world.chooseTrayItem("Exit"))).toBe(true);
  await waitFor("the app to exit", () =>
    Promise.resolve(world.applicationProcesses(APP).length === 0),
  );
  await app.stop();
}

/** The current window's visible text. */
function pageText(app: RunningApp): Promise<string> {
  return app.session.evaluate<string>("return document.body.innerText");
}

/** Names of every folder called `name` anywhere under `root`. */
function foldersNamed(root: string, name: string): string[] {
  const found: string[] = [];
  const walk = (directory: string, depth: number): void => {
    if (depth > 4) return;
    for (const entry of readdirSync(directory)) {
      const path = join(directory, entry);
      try {
        if (!statSync(path).isDirectory()) continue;
      } catch {
        continue;
      }
      if (entry === name) found.push(path);
      walk(path, depth + 1);
    }
  };
  walk(root, 0);
  return found;
}

describe("the real application", () => {
  it("first launch: an empty profile opens the overview with the first-launch screen", async () => {
    const world = await begin("first-launch");
    const app = await startOverview(world, APP);
    await waitFor("the first-launch screen", async () =>
      (await pageText(app)).includes("Add your first account") ? true : null,
    );
    await world.screenshot(app.session, "first-launch");
    // The profile was created on first launch, inside the sandbox.
    expect(existsSync(join(world.config, DEV_IDENTIFIER, "quota.sqlite"))).toBe(true);
    await app.stop();
  });

  it("the main popover appears at 440 px with its header and footer", async () => {
    const world = await begin("popover");
    const app = await startOverview(world, APP);
    await waitFor("the footer", async () =>
      (await pageText(app)).includes("Local only · monitoring active") ? true : null,
    );
    const text = await pageText(app);
    expect(text).toContain("Your AI subscriptions");
    expect(await app.session.evaluate<number>("return window.innerWidth")).toBe(440);
    await world.screenshot(app.session, "popover");
    await app.stop();
  });

  it("settings open from the popover and a preference persists across a restart", async () => {
    const world = await begin("settings-persist");
    let app = await startOverview(world, APP);
    const before = await app.session.evaluate<string>(
      "return document.documentElement.dataset.theme",
    );
    const chosen = before === "dark" ? "Light" : "Dark";

    await (await app.session.find('[aria-label="Settings"]')).click();
    await app.session.switchTo(await findWindow(app.session, "settings"));
    await waitUntilVisible(app.session, "the settings window");
    await (await app.session.findByText("button", "Appearance")).click();
    await (await app.session.findByText("button", chosen)).click();
    // The theme changes only once the host has confirmed and saved the choice.
    await waitFor("the confirmed theme", async () =>
      (await app.session.evaluate<string>(
        "return document.documentElement.dataset.theme",
      )) === chosen.toLowerCase()
        ? true
        : null,
    );
    await world.screenshot(app.session, "settings-appearance");
    await quit(world, app);

    app = await startOverview(world, APP);
    // The renderer paints the system theme until the host publishes the saved
    // preferences, so the saved choice is waited for rather than read at once.
    await waitFor("the saved theme after the restart", async () =>
      (await app.session.evaluate<string>(
        "return document.documentElement.dataset.theme",
      )) === chosen.toLowerCase()
        ? true
        : null,
    );
    await app.stop();
  });

  it("privacy aliases replace account names everywhere", async () => {
    const world = await begin("privacy-alias");
    const app = await startOverview(world, SAMPLE_APP);
    await waitFor("the sample accounts", async () =>
      (await app.session.evaluate<number>(
        "return document.querySelectorAll('.provider-card').length",
      )) >= 3
        ? true
        : null,
    );
    const names = await app.session.evaluate<string[]>(
      "return [...document.querySelectorAll('.provider-card .provider-meta')].map((e) => e.textContent.trim())",
    );
    expect(names.length).toBeGreaterThan(0);
    expect(names.some((name) => /^Account \d+$/.test(name))).toBe(false);
    await world.screenshot(app.session, "sample-accounts");

    await (await app.session.find('[aria-label="Settings"]')).click();
    await app.session.switchTo(await findWindow(app.session, "settings"));
    await waitUntilVisible(app.session, "the settings window");
    await (await app.session.findByText("button", "Privacy")).click();
    await (await app.session.find('[aria-label="Hide account names"]')).click();

    await app.session.switchTo(await findWindow(app.session, "overview"));
    await waitFor("aliases in the overview", async () => {
      const shown = await app.session.evaluate<string[]>(
        "return [...document.querySelectorAll('.provider-card .provider-meta')].map((e) => e.textContent.trim())",
      );
      return shown.length === names.length &&
        shown.every((name) => /^Account \d+$/.test(name))
        ? shown
        : null;
    });
    const text = await pageText(app);
    for (const name of names) expect(text).not.toContain(name);
    await world.screenshot(app.session, "sample-accounts-aliased");
    await app.stop();
  });

  it("a second launch does not start a second app and brings the first forward", async () => {
    const world = await begin("single-instance");
    const app = await startOverview(world, APP);
    expect(world.applicationProcesses(APP)).toHaveLength(1);

    // Put the popover away, so that being brought forward is observable.
    await (await app.session.find('[aria-label="Hide popover"]')).click();
    await waitFor("the overview to hide", async () =>
      (await app.session.evaluate<string>("return document.visibilityState")) === "hidden"
        ? true
        : null,
    );

    const second = world.launchDirect(APP);
    const exit = await new Promise<number | null>((resolve) => {
      second.once("exit", (code) => {
        resolve(code);
      });
    });
    expect(exit).toBe(0);
    expect(world.applicationProcesses(APP)).toHaveLength(1);
    await waitUntilVisible(app.session, "the overview after the second launch");
    await app.stop();
  });

  // WebDriver cannot operate the native Windows tray menu.
  (process.platform === "win32" ? it.skip : it)(
    "quitting from the tray menu ends the app and leaves nothing behind",
    async () => {
      const world = await begin("clean-quit");
      const app = await startOverview(world, APP);
      expect(world.applicationProcesses(APP)).toHaveLength(1);
      expect(await world.busNames()).toContain(`${DEV_IDENTIFIER}.SingleInstance`);

      const chosen = await waitFor("the tray menu", () => world.chooseTrayItem("Exit"));
      expect(chosen).toBe(true);
      await waitFor("the app to exit", () =>
        Promise.resolve(world.applicationProcesses(APP).length === 0),
      );
      // The single-instance lock is released, and no helper process is left.
      await waitFor("the single-instance name to be released", async () =>
        (await world.busNames()).includes(`${DEV_IDENTIFIER}.SingleInstance`)
          ? null
          : true,
      );
      expect(
        world
          .leftoverProcesses()
          .filter((name) => name.startsWith("WebKit") && name !== "WebKitWebDriver"),
      ).toEqual([]);
      await app.session.end().catch(() => undefined);
    },
  );

  it("a debug build runs under the Quota Dev identity and writes only its own folders", async () => {
    const world = await begin("dev-identity");
    const app = await startOverview(world, APP);
    await waitFor("the profile", () =>
      Promise.resolve(existsSync(join(world.config, DEV_IDENTIFIER, "quota.sqlite"))),
    );
    // The development identifier names every folder the app created.
    expect(existsSync(join(world.config, DEV_IDENTIFIER))).toBe(true);
    expect(existsSync(join(world.data, DEV_IDENTIFIER))).toBe(true);
    expect(foldersNamed(world.root, PRODUCTION_IDENTIFIER)).toEqual([]);
    if (process.platform === "linux") {
      expect(await world.busNames()).toContain(`${DEV_IDENTIFIER}.SingleInstance`);
      expect(await world.busNames()).not.toContain(
        `${PRODUCTION_IDENTIFIER}.SingleInstance`,
      );
    } else {
      expect(world.applicationProcesses(APP)).toHaveLength(1);
    }
    await app.stop();
  });

  it("adds an account against a fake local provider and shows its reading", async () => {
    const world = await begin("add-account");
    const provider = await startFakeProvider();
    try {
      // The sandbox user is signed in to Codex, as the Codex CLI would leave it.
      world.writeCodexSignIn("fake-codex-token");
      const app = await world.launch(SAMPLE_APP, {
        QUOTA_E2E_PROVIDER_BASE: provider.base,
      });
      await app.session.switchTo(await findWindow(app.session, "overview"));
      await waitUntilVisible(app.session, "the overview");
      await waitFor("the sample accounts", async () =>
        (await app.session.evaluate<number>(
          "return document.querySelectorAll('.provider-card').length",
        )) >= 3
          ? true
          : null,
      );

      // Add account in the popover opens the wizard in the settings window.
      await (await app.session.findByText("button", "Add account")).click();
      await app.session.switchTo(await findWindow(app.session, "settings"));
      await waitUntilVisible(app.session, "the settings window");
      await (
        await app.session.find(
          "//button[contains(@class,'provider-pick')][.//span[normalize-space()='Codex']]",
          "xpath",
        )
      ).click();
      await (await app.session.findByText("button", "Connect")).click();
      await waitFor("the verified account", async () =>
        (await pageText(app)).includes("Add this account?") ? true : null,
      );
      // The application really asked the fake provider, with the sandbox's token.
      expect(provider.requests.map((request) => request.path)).toContain(
        "/backend-api/wham/usage",
      );
      expect(provider.requests.at(0)?.authorization).toBe("Bearer fake-codex-token");
      await world.screenshot(app.session, "add-account-verify");
      await (await app.session.findByText("button", "Add Codex account")).click();

      // The new account is in the overview with the fake provider's reading:
      // 28% used of the 5-hour window leaves 72%.
      await app.session.switchTo(await findWindow(app.session, "overview"));
      const card = await waitFor("the Codex card", async () => {
        const text = await app.session.evaluate<string | null>(
          "const card = [...document.querySelectorAll('.provider-card')].find((c) => c.getAttribute('aria-label')?.startsWith('Codex')); return card ? card.innerText : null",
        );
        return text !== null && text.includes("72") ? text : null;
      });
      expect(card).toContain("72");
      await world.screenshot(app.session, "add-account-overview");
      await app.stop();
    } finally {
      await provider.close();
    }
  });
});
