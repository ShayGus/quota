/**
 * The Playwright harness for the interface tests.
 *
 * `open` loads the real built renderer in headless Chromium with the faked host
 * injected ahead of it. The clock is fixed so relative times ("resets in 2h")
 * and screenshots are the same on every run. After each test the harness fails
 * the test if the page threw, logged an error, or asked the faked host for a
 * command it does not know, so a silent regression cannot pass.
 */
import { mkdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import process from "node:process";

import { expect, test as base, type Locator, type Page } from "@playwright/test";

import type { RecordedCall, FakeConfig } from "./fake-backend";
import { FAKE_BACKEND_DIRECTORY, FAKE_BACKEND_FILE } from "./paths";
import { NOW } from "./scenarios";

/** The window sizes the application creates, in CSS pixels. */
export const WINDOW_SIZE = {
  overview: { width: 440, height: 640 },
  settings: { width: 780, height: 600 },
  widget: { width: 316, height: 172 },
} as const;

/** A loaded window and what the test can do with the faked host behind it. */
export interface Host {
  readonly page: Page;
  /** Every IPC call the renderer made, in order. */
  calls: () => Promise<RecordedCall[]>;
  /** The calls to one command. */
  callsTo: (command: string) => Promise<RecordedCall[]>;
  /** Publishes a host event, as the Rust side does. */
  emit: (event: string, payload: unknown) => Promise<void>;
  /** The faked host's current snapshot and preferences. */
  hostState: () => Promise<{
    snapshot: FakeConfig["snapshot"];
    preferences: FakeConfig["preferences"];
  }>;
  /**
   * Saves a screenshot under `test-results/screenshots/<name>.png`, which CI
   * uploads. `name` may contain folders. With a locator, only that element.
   */
  screenshot: (name: string, element?: Locator) => Promise<void>;
}

interface Opener {
  /**
   * Loads one window of the application. A second window of the same test goes
   * in `target`, a page of the same browser context.
   */
  open: (config: FakeConfig, hash?: string, target?: Page) => Promise<Host>;
  /** A second page in the test's browser context, for a second window. */
  secondWindow: () => Promise<Page>;
}

interface Fixtures {
  windows: Opener;
  open: Opener["open"];
  secondWindow: Opener["secondWindow"];
}

/** Where screenshots are kept for CI to upload. */
const SCREENSHOT_DIRECTORY = join(process.cwd(), "test-results", "screenshots");

const fakeBackendScript = (): string =>
  readFileSync(join(FAKE_BACKEND_DIRECTORY, FAKE_BACKEND_FILE), "utf8");

export const test = base.extend<Fixtures>({
  open: async ({ windows }, provide) => {
    await provide(windows.open);
  },
  secondWindow: async ({ windows }, provide) => {
    await provide(windows.secondWindow);
  },
  windows: async ({ page, context }, provide) => {
    const problems: string[] = [];
    const opened: Page[] = [];
    const watch = (target: Page): void => {
      target.on("pageerror", (error) => {
        problems.push(`page error: ${error.message}`);
      });
      target.on("console", (message) => {
        if (message.type() === "error") {
          problems.push(`console error: ${message.text()}`);
        }
      });
    };
    watch(page);

    const open = async (config: FakeConfig, hash = "", target = page): Promise<Host> => {
      opened.push(target);
      await target.setViewportSize(WINDOW_SIZE[config.window]);
      await target.clock.setFixedTime(NOW);
      await target.addInitScript({ content: fakeBackendScript() });
      await target.addInitScript((installed) => {
        window.__installQuotaFake?.(installed);
      }, config);
      await target.goto(`/index.html${hash}`);
      return {
        page: target,
        calls: () => target.evaluate(() => [...(window.__quotaFake?.calls ?? [])]),
        callsTo: (command) =>
          target.evaluate(
            (name) => (window.__quotaFake?.calls ?? []).filter((c) => c.command === name),
            command,
          ),
        emit: (event, payload) =>
          target.evaluate(([name, body]) => window.__quotaFake?.emit(name, body), [
            event,
            payload,
          ] as const),
        hostState: () =>
          target.evaluate(() => {
            const state = window.__quotaFake?.state();
            if (state === undefined) throw new Error("the faked host is not installed");
            return state;
          }),
        screenshot: async (name, element) => {
          // Written outside Playwright's per-test output folder, which is
          // emptied for a passing test. CI uploads the folder.
          const path = join(SCREENSHOT_DIRECTORY, `${name}.png`);
          mkdirSync(dirname(path), { recursive: true });
          await (element ?? target).screenshot({ path });
        },
      };
    };

    await provide({
      open,
      secondWindow: async () => {
        const next = await context.newPage();
        watch(next);
        return next;
      },
    });

    for (const target of opened) {
      const unhandled = await target.evaluate(() => window.__quotaFake?.unhandled ?? []);
      expect(unhandled, "commands the faked host does not know").toEqual([]);
    }
    expect(problems, "errors the page reported").toEqual([]);
  },
});

export { expect };
