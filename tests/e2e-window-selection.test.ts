import { describe, expect, it, vi } from "vitest";

import { findWindow } from "./e2e/sandbox";
import { Session } from "./e2e/webdriver";

interface WindowDocument {
  readonly href: string;
  readonly body: string;
}

/** Execute the selector's browser scripts against each window's actual DOM. */
function windows(documents: Record<string, WindowDocument>) {
  const session = new Session("http://unused.test", "session");
  let current = "";
  vi.spyOn(session, "handles").mockResolvedValue(Object.keys(documents));
  vi.spyOn(session, "switchTo").mockImplementation((handle) => {
    current = handle;
    return Promise.resolve();
  });
  vi.spyOn(session, "evaluate").mockImplementation(
    <T>(script: string, args: unknown[] = []): Promise<T> => {
      const page = documents[current];
      if (page === undefined) throw new Error("no current window");
      document.body.innerHTML = page.body;
      return Promise.resolve(
        runInNewContext(`(function() { ${script} })(...args)`, {
          document,
          window: { location: { href: page.href } },
          args,
        }) as T,
      );
    },
  );
  return { session, current: () => current };
}

describe("real-app window selection", () => {
  it.each(["overview", "settings", "widget"] as const)(
    "selects the rendered %s among blank, loading, and sibling windows",
    async (role) => {
      const browser = windows({
        blank: { href: "about:blank", body: "" },
        loading: {
          href: "https://tauri.localhost/index.html",
          body: '<div id="root"></div>',
        },
        update: {
          href: "https://tauri.localhost/index.html#/update",
          body: '<div class="update-window"></div>',
        },
        overview: {
          href: "https://tauri.localhost/index.html",
          body: '<div class="window popover"><button aria-label="Settings"></button></div>',
        },
        settings: {
          href: "https://tauri.localhost/index.html#/settings/accounts",
          body: '<div class="window settings-window"></div>',
        },
        widget: {
          href: "https://tauri.localhost/index.html#/widget",
          body: '<div class="widget"></div>',
        },
      });
      expect(await findWindow(browser.session, role)).toBe(role);
      expect(browser.current()).toBe(role);
    },
  );

  it.each([
    ["overview", "", "popover"],
    ["settings", "#/settings", "settings-window"],
    ["widget", "#/widget", "widget"],
  ] as const)(
    "waits for the %s renderer after its URL loads",
    async (role, hash, surface) => {
      vi.useFakeTimers();
      const documents = {
        loading: {
          href: `tauri://localhost/index.html${hash}`,
          body: '<div id="root"></div>',
        },
      };
      const browser = windows(documents);
      let selected = false;
      const selection = findWindow(browser.session, role).then((handle) => {
        selected = true;
        return handle;
      });
      await vi.advanceTimersByTimeAsync(0);
      expect(selected).toBe(false);
      documents.loading.body = `<div class="${surface}"></div>`;
      await vi.advanceTimersByTimeAsync(250);
      expect(await selection).toBe("loading");
    },
  );
});
import { runInNewContext } from "node:vm";
