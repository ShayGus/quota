/**
 * The compact mini widget in the real application: the window really fits its
 * content, really snaps to a screen's edge, and really draws its rings around
 * itself. The interface tests fake the host, so they can't tell any of that.
 *
 * Everything goes through WebDriver, inside a throwaway profile, so these run
 * on CI's machines and touch no real mouse or keyboard.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";

import { afterEach, describe, expect, it } from "vitest";

import { moveNativeWindow, nativeWindow, type NativeWindow } from "./native-window";
import {
  DEV_IDENTIFIER,
  findWindow,
  Sandbox,
  waitUntilVisible,
  type RunningApp,
} from "./sandbox";
import { waitFor } from "./webdriver";

const EXE = process.platform === "win32" ? ".exe" : "";
const SAMPLE_APP =
  process.env["QUOTA_E2E_SAMPLE_APP"] ??
  join("src-tauri", "target", "e2e", `quota-sample${EXE}`);

/** The list's widest, and how far the rings reach, from `platform/`. */
const WIDEST = 400;
const HALO_REACH = 46;

/** A little longer than `HALO_LASTS` in `platform/widget_rings.rs`. */
const RINGS_LAST_MS = 3500;

/** The debug build's window titles, from `tauri.dev.conf.json` and `platform/widget.rs`. */
const WIDGET_TITLE = "Quota Dev widget";
const RINGS_TITLE = "Quota widget rings";

let sandbox: Sandbox | null = null;

afterEach(async () => {
  await sandbox?.destroy();
  sandbox = null;
});

/** Opens the sample accounts as the compact list, chosen through the app's own UI. */
async function startList(name: string): Promise<{ world: Sandbox; app: RunningApp }> {
  const world = await Sandbox.create(name);
  sandbox = world;
  const app = await world.launch(SAMPLE_APP);
  const { session } = app;
  const overview = await findWindow(session, "overview");
  await session.switchTo(overview);
  await waitUntilVisible(session, "the overview");
  await (await session.find('[aria-label="Settings"]')).click();
  await session.switchTo(await findWindow(session, "settings"));
  const appearance = await waitFor("the settings", () =>
    session.findByText("button", "Appearance"),
  );
  await appearance.click();
  // The panel draws after the click, so it may not be there yet.
  const compact = await waitFor("the appearance panel", () =>
    session.findByText("button", "Compact"),
  );
  await compact.click();
  await session.switchTo(overview);
  await (await session.find('[aria-label="Switch to the mini widget"]')).click();
  await session.switchTo(await findWindow(session, "widget"));
  await waitFor("the list", () =>
    session.evaluate<boolean>("return document.querySelector('.widget-row') !== null"),
  );
  return { world, app };
}

/**
 * Waits for the widget's flash, then for the rings that come with it to be gone,
 * and that the app is still there. Seeing the flash alone says nothing about
 * them: the app can still die drawing the rings.
 */
async function flashesAndSurvives(world: Sandbox, app: RunningApp): Promise<void> {
  await waitFor("the flash", () =>
    app.session.evaluate<boolean>(
      'return document.querySelector(".widget.flashing") !== null',
    ),
  );
  await new Promise((resolve) => {
    setTimeout(resolve, RINGS_LAST_MS);
  });
  expect(
    await app.session.evaluate<boolean>(
      'return document.querySelector(".widget") !== null',
    ),
  ).toBe(true);
  expect(world.applicationProcesses(SAMPLE_APP)).toHaveLength(1);
}

/** The list's own size, and the window's, as the page measures them. */
function sizes(app: RunningApp) {
  return app.session.evaluate<{
    readonly width: number;
    readonly height: number;
    readonly windowWidth: number;
    readonly windowHeight: number;
  }>(`
    const box = document.querySelector(".widget").getBoundingClientRect();
    return {
      width: Math.ceil(box.width),
      height: Math.round(box.height),
      windowWidth: window.innerWidth,
      windowHeight: window.innerHeight,
    };
  `);
}

/** Waits until the window has settled exactly around the content. */
function fitted(app: RunningApp, what: string) {
  return waitFor(`the window to fit ${what}`, async () => {
    const seen = await sizes(app);
    return Math.abs(seen.windowWidth - seen.width) <= 1 &&
      Math.abs(seen.windowHeight - seen.height) <= 1
      ? seen
      : null;
  });
}

/** How many device pixels one CSS pixel is, which window positions are in. */
function scaleOf(app: RunningApp): Promise<number> {
  return app.session.evaluate<number>("return window.devicePixelRatio");
}

/** The widget's window, as Windows has it. */
function widgetWindow(): NativeWindow {
  const found = nativeWindow(WIDGET_TITLE);
  if (found === null) throw new Error("the widget's window isn't showing");
  return found;
}

/** Waits until the widget's window passes `check`. */
function settled(what: string, check: (window: NativeWindow) => boolean) {
  return waitFor(what, () => {
    const found = nativeWindow(WIDGET_TITLE);
    if (found !== null && check(found)) return Promise.resolve(found);
    // Says where the window was, if this runs out of time.
    throw new Error(`the window was ${JSON.stringify(found)}`);
  });
}

describe("the compact mini widget", () => {
  it("fits its window to the list", async () => {
    const { world, app } = await startList("widget-fit");
    const seen = await fitted(app, "the list");
    expect(seen.width).toBeLessThanOrEqual(WIDEST);
    await world.screenshot(app.session, "widget-list");
    await app.stop();
  });

  it("doesn't save where it first opens as a spot the person chose", async () => {
    const { world, app } = await startList("widget-unsaved");
    await fitted(app, "the list");
    // Longer than the host waits before it saves a move.
    await new Promise((resolve) => setTimeout(resolve, 1500));
    const saved = readFileSync(
      join(world.config, DEV_IDENTIFIER, "sample", "preferences.json"),
      "utf8",
    );
    expect(saved).toMatch(/"widget_position":\s*null/);
    await app.stop();
  });

  it("is see-through at rest, and solid when pointed at", async () => {
    const { world, app } = await startList("widget-solid");
    const { session } = app;
    const fill = (): Promise<string> =>
      session.evaluate<string>(
        'return getComputedStyle(document.querySelector(".widget-list")).backgroundColor',
      );
    await waitFor("the see-through background", async () =>
      (await fill()).includes("0.75") ? true : null,
    );
    await session.pointAt(await session.find(".widget-row"));
    await waitFor("the solid background", async () =>
      (await fill()).startsWith("rgb(") ? true : null,
    );
    await world.screenshot(app.session, "widget-pointed-at");
    await app.stop();
  });

  it("opens a row's details under it, without getting wider", async () => {
    const { world, app } = await startList("widget-details");
    const { session } = app;
    const rest = await fitted(app, "the list");
    await (await session.find(".widget-row")).click();
    await waitFor("the details", () =>
      session.evaluate<boolean>(
        'return document.querySelector(".widget-details") !== null',
      ),
    );
    const open = await waitFor("the window to grow", async () => {
      const seen = await fitted(app, "the details");
      return seen.height > rest.height ? seen : null;
    });
    expect(open.width).toBe(rest.width);
    await world.screenshot(app.session, "widget-details");
    await (await session.find(".widget-row")).click();
    await waitFor("the window to shrink back", async () => {
      const seen = await fitted(app, "the list");
      return seen.height === rest.height ? true : null;
    });
    await app.stop();
  });

  // Windows only: these ask the system where windows are and move them, which
  // WebDriver can't do. The geometry itself has unit tests on every system, in
  // `platform/widget_tests.rs`.
  describe.runIf(process.platform === "win32")("on the desktop", () => {
    it("opens details upward near the bottom, and keeps its bottom edge", async () => {
      const { world, app } = await startList("widget-upward");
      await fitted(app, "the list");
      const scale = await scaleOf(app);
      const before = widgetWindow();
      // Well away from the edge, so it stays exactly there and doesn't snap.
      const y = before.area.bottom - before.height - 60 * scale;
      moveNativeWindow(WIDGET_TITLE, before.x, y);
      const placed = await settled(
        "the widget to go near the bottom",
        (seen) => Math.abs(seen.y - y) <= 1,
      );
      await (await app.session.find(".widget-row")).click();
      await waitFor("the details", () =>
        app.session.evaluate<boolean>(
          'return document.querySelector(".widget-details") !== null',
        ),
      );
      const open = await settled(
        "the details to open upward",
        (seen) => seen.height > placed.height && seen.y < placed.y,
      );
      expect(
        Math.abs(open.y + open.height - (placed.y + placed.height)),
      ).toBeLessThanOrEqual(2);
      await world.screenshot(app.session, "widget-details-upward");
      await app.stop();
    });

    it("stays where it was left when details open straight after the move", async () => {
      const { world, app } = await startList("widget-quick-open");
      await fitted(app, "the list");
      const scale = await scaleOf(app);
      const before = widgetWindow();
      const y = before.area.bottom - before.height - 60 * scale;
      moveNativeWindow(WIDGET_TITLE, before.x, y);
      // No waiting here: the host decides a move has rested half a second after
      // it, and the details have to open before then.
      await (await app.session.find(".widget-row")).click();
      await settled(
        "the details to open upward",
        (seen) => seen.height > before.height && seen.y < y,
      );
      // Longer than the host waits before it decides, so a snap would have happened.
      await new Promise((resolve) => setTimeout(resolve, 1500));
      const open = widgetWindow();
      expect(Math.abs(open.y + open.height - (y + before.height))).toBeLessThanOrEqual(2);
      const saved = readFileSync(
        join(world.config, DEV_IDENTIFIER, "sample", "preferences.json"),
        "utf8",
      );
      expect(saved).toMatch(
        new RegExp(`"widget_position":\\s*\\{[^}]*"y":\\s*${String(y)}\\b`),
      );
      await app.stop();
    });

    it("snaps to a screen's edge when left near it, and stays where it's left", async () => {
      const { app } = await startList("widget-snap");
      await fitted(app, "the list");
      const scale = await scaleOf(app);
      const margin = 8 * scale;
      const { area, width } = widgetWindow();
      const y = area.top + 200 * scale;

      const x = area.left + 300 * scale;
      moveNativeWindow(WIDGET_TITLE, x, y);
      // Longer than the host waits before it decides, so a snap would have happened.
      await new Promise((resolve) => setTimeout(resolve, 1500));
      expect(Math.abs(widgetWindow().x - x)).toBeLessThanOrEqual(1);

      moveNativeWindow(WIDGET_TITLE, area.left + 15 * scale, y);
      await settled(
        "the widget to snap left",
        (seen) => Math.abs(seen.x - (area.left + margin)) <= 2,
      );

      moveNativeWindow(WIDGET_TITLE, area.right - width - 15 * scale, y);
      await settled(
        "the widget to snap right",
        (seen) => Math.abs(seen.x + seen.width - (area.right - margin)) <= 2,
      );
      await app.stop();
    });

    it("draws its rings centred around itself and takes them away again", async () => {
      const { app } = await startList("widget-rings");
      await fitted(app, "the list");
      const scale = await scaleOf(app);
      const widget = widgetWindow();
      await app.session.evaluateAsync(
        `const done = arguments[arguments.length - 1];
         window.__TAURI_INTERNALS__.invoke("show_widget_halo").then(done, done);`,
      );
      const reach = HALO_REACH * scale;
      const rings = await waitFor("the rings", () =>
        Promise.resolve(nativeWindow(RINGS_TITLE)),
      );
      expect(Math.abs(rings.x - (widget.x - reach))).toBeLessThanOrEqual(2);
      expect(Math.abs(rings.y - (widget.y - reach))).toBeLessThanOrEqual(2);
      expect(Math.abs(rings.width - (widget.width + 2 * reach))).toBeLessThanOrEqual(2);
      expect(Math.abs(rings.height - (widget.height + 2 * reach))).toBeLessThanOrEqual(2);
      await waitFor("the rings to go", () =>
        Promise.resolve(nativeWindow(RINGS_TITLE) === null ? true : null),
      );
      await app.stop();
    });
  });

  it("flashes when Quota is opened again, as a click on the tray icon does", async () => {
    const { world, app } = await startList("widget-activate");
    const second = world.launchDirect(SAMPLE_APP);
    await new Promise((resolve) => {
      second.once("exit", resolve);
    });
    await flashesAndSurvives(world, app);
    await app.stop();
  });

  it("offers the shortcut that finds it, and lets it be turned off", async () => {
    const world = await Sandbox.create("widget-shortcut");
    sandbox = world;
    const app = await world.launch(SAMPLE_APP);
    const { session } = app;
    await session.switchTo(await findWindow(session, "overview"));
    await waitUntilVisible(session, "the overview");
    await (await session.find('[aria-label="Settings"]')).click();
    await session.switchTo(await findWindow(session, "settings"));
    const shortcut = await waitFor("the shortcut setting", () =>
      session.find('[aria-label^="Change the shortcut"]'),
    );
    expect(await shortcut.text()).toBe("Ctrl+Alt+Q");
    await world.screenshot(session, "settings-find-shortcut");
    await (await session.findByText("button", "Turn off")).click();
    await waitFor("the shortcut to be off", async () =>
      (await shortcut.text()) === "Off" ? true : null,
    );
    await (await session.findByText("button", "Turn on")).click();
    await waitFor("the shortcut to be back", async () =>
      (await shortcut.text()) === "Ctrl+Alt+Q" ? true : null,
    );
    await app.stop();
  });

  it.runIf(process.platform === "linux")(
    "flashes from the tray's Find widget",
    async () => {
      const { world, app } = await startList("widget-find");
      expect(
        await waitFor("the tray menu", () => world.chooseTrayItem("Find widget")),
      ).toBe(true);
      await flashesAndSurvives(world, app);
      await app.stop();
    },
  );
});
