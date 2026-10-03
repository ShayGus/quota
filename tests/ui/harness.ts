/**
 * The Playwright harness for the interface tests.
 *
 * `open` loads the real built renderer in headless Chromium with the faked host
 * injected ahead of it. The clock is fixed so relative times ("resets in 2h")
 * and screenshots are the same on every run. After each test the harness fails
 * the test if the page threw, logged an error, or asked the faked host for a
 * command it does not know, so a silent regression cannot pass.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";

import { expect, test as base, type Page } from "@playwright/test";

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
  /** Saves a screenshot as a test attachment, which CI uploads. */
  screenshot: (name: string) => Promise<void>;
}

interface Fixtures {
  open: (config: FakeConfig, hash?: string) => Promise<Host>;
}

/** Where screenshots are kept for CI to upload. */
const SCREENSHOT_DIRECTORY = join(process.cwd(), "test-results", "screenshots");

const fakeBackendScript = (): string =>
  readFileSync(join(FAKE_BACKEND_DIRECTORY, FAKE_BACKEND_FILE), "utf8");

export const test = base.extend<Fixtures>({
  open: async ({ page }, provide, testInfo) => {
    const problems: string[] = [];
    page.on("pageerror", (error) => {
      problems.push(`page error: ${error.message}`);
    });
    page.on("console", (message) => {
      if (message.type() === "error") {
        problems.push(`console error: ${message.text()}`);
      }
    });

    const session = { opened: false };
    await provide(async (config, hash = "") => {
      session.opened = true;
      await page.setViewportSize(WINDOW_SIZE[config.window]);
      await page.clock.setFixedTime(NOW);
      await page.addInitScript({ content: fakeBackendScript() });
      await page.addInitScript((installed) => {
        window.__installQuotaFake?.(installed);
      }, config);
      await page.goto(`/index.html${hash}`);
      const host: Host = {
        page,
        calls: () => page.evaluate(() => [...(window.__quotaFake?.calls ?? [])]),
        callsTo: (command) =>
          page.evaluate(
            (name) => (window.__quotaFake?.calls ?? []).filter((c) => c.command === name),
            command,
          ),
        emit: (event, payload) =>
          page.evaluate(([name, body]) => window.__quotaFake?.emit(name, body), [
            event,
            payload,
          ] as const),
        hostState: () =>
          page.evaluate(() => {
            const state = window.__quotaFake?.state();
            if (state === undefined) throw new Error("the faked host is not installed");
            return state;
          }),
        screenshot: async (name) => {
          // Written beside the traces, outside Playwright's per-test output
          // folder, which is emptied for a passing test. CI uploads the folder.
          const path = join(SCREENSHOT_DIRECTORY, `${name}.png`);
          await page.screenshot({ path });
          await testInfo.attach(name, { path, contentType: "image/png" });
        },
      };
      return host;
    });

    if (session.opened) {
      const unhandled = await page.evaluate(() => window.__quotaFake?.unhandled ?? []);
      expect(unhandled, "commands the faked host does not know").toEqual([]);
    }
    expect(problems, "errors the page reported").toEqual([]);
  },
});

export { expect };
