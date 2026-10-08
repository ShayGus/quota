/**
 * The display modes, one test for every combination that exists.
 *
 * `matrix.ts` explains which combinations those are. Each test loads one surface
 * with one account set, theme and privacy setting, checks what the renderer
 * draws, and saves a screenshot under `test-results/screenshots/matrix/`.
 *
 * - counts: every surface with 0, 1, 2, 3 and 7 accounts, in both themes, with
 *   privacy aliases off and on;
 * - states: every surface with one account in each state, in both themes;
 * - long names and sizes, and the switches between the full window and widget.
 */
import type { Page } from "@playwright/test";

import { account, percent, window as quotaWindow } from "../fixtures";
import { expect, test } from "./harness";
import {
  accountIn,
  COUNTS,
  mixedAccounts,
  NICKNAME_MARK,
  STATES,
  SURFACES,
  surfacePreferences,
  THEMES,
  type AccountState,
  type Surface,
} from "./matrix";
import { defaultPreferences, scenario } from "./scenarios";
import type { FakeConfig } from "./fake-backend";
import type { AccountSnapshot } from "../../src/generated/bindings";

/** The mini widget's width, from `.widget` in widget.css. */
const WIDGET_WIDTH = 316;

/** The hash each webview is opened with. */
const HASH = { overview: "", widget: "#/widget" } as const;

/** A configuration for one surface. */
function configFor(
  surface: Surface,
  accounts: readonly AccountSnapshot[],
  theme: "light" | "dark",
  alias: boolean,
): FakeConfig {
  const base = defaultPreferences();
  return scenario(surface.window, {
    accounts,
    preferences: surfacePreferences(surface, base, {
      theme,
      privacy: { ...base.privacy, alias_mode: alias ? "stable_aliases" : "off" },
    }),
  });
}

/** The element a surface draws its content in, which is what is photographed. */
function canvas(page: Page, surface: Surface) {
  return surface.window === "widget" ? page.locator(".widget") : page.locator(".popover");
}

/** What a surface shows for `count` accounts, as the DOM states it. */
async function shown(page: Page, surface: Surface) {
  return page.evaluate((id) => {
    const count = (selector: string): number =>
      document.querySelectorAll(selector).length;
    const box = (selector: string) =>
      document.querySelector(selector)?.getBoundingClientRect();
    return {
      id,
      cards: count(".provider-card"),
      tiles: count(".widget-tile"),
      miniCards: count(".widget-card"),
      wide: count(".widget-card.wide"),
      empty: count(".widget-empty") + count(".empty"),
      scrollWidth: document.documentElement.scrollWidth,
      innerWidth: window.innerWidth,
      widget: box(".widget")?.width,
      lastCard: [...document.querySelectorAll(".widget-card")]
        .at(-1)
        ?.getBoundingClientRect().width,
      container: document.querySelector(".widget-cards")?.getBoundingClientRect().width,
      tileRows: [
        ...new Set(
          [...document.querySelectorAll(".widget-tile")].map(
            (t) => (t as HTMLElement).offsetTop,
          ),
        ),
      ].length,
      tileRowSizes: Object.values(
        [...document.querySelectorAll(".widget-tile")].reduce<Record<number, number>>(
          (rows, tile) => {
            const top = (tile as HTMLElement).offsetTop;
            rows[top] = (rows[top] ?? 0) + 1;
            return rows;
          },
          {},
        ),
      ),
      pinned: count(".popover.pinned"),
      pinLabel: count(".pin-label"),
      theme: document.documentElement.dataset["theme"],
      html: document.body.innerHTML,
    };
  }, surface.id);
}

test.describe("counts", () => {
  for (const surface of SURFACES) {
    for (const count of COUNTS) {
      for (const theme of THEMES) {
        for (const alias of [false, true]) {
          const label = `${surface.id}-n${String(count)}-${theme}-${alias ? "alias" : "names"}`;
          test(`${surface.name}: ${String(count)} accounts, ${theme}, ${alias ? "aliases on" : "aliases off"}`, async ({
            open,
          }) => {
            const accounts = mixedAccounts(count);
            const host = await open(
              configFor(surface, accounts, theme, alias),
              HASH[surface.window],
            );
            const { page } = host;
            if (count === 0) {
              await expect(
                surface.window === "widget"
                  ? page.getByText("No accounts to show")
                  : page.getByText("Add your first account").first(),
              ).toBeVisible();
            } else if (surface.window === "overview") {
              await expect(page.locator(".provider-card")).toHaveCount(count);
            } else if (surface.style === "ring") {
              await expect(page.locator(".widget-tile")).toHaveCount(count);
            } else {
              await expect(page.locator(".widget-card")).toHaveCount(count);
            }
            await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
            const seen = await shown(page, surface);
            expect(seen.scrollWidth).toBeLessThanOrEqual(seen.innerWidth);

            if (surface.window === "overview") {
              expect(seen.pinned).toBe(surface.mode === "floating" ? 1 : 0);
              expect(seen.pinLabel).toBe(surface.mode === "floating" ? 1 : 0);
            } else {
              expect(seen.widget).toBe(WIDGET_WIDTH);
            }
            if (surface.style === "bar" && surface.window === "widget" && count > 0) {
              // Two cards a row; an odd last card takes the whole width.
              expect(seen.wide).toBe(count % 2);
              if (count % 2 === 1) {
                expect(seen.lastCard).toBe(
                  seen.container ? seen.container - 18 : undefined,
                );
              }
            }
            if (surface.style === "ring" && surface.window === "widget" && count > 0) {
              // At most four tiles a row, rows as even as they can be.
              expect(seen.tileRows).toBe(Math.ceil(count / 4));
              const sizes = seen.tileRowSizes;
              expect(Math.max(...sizes) - Math.min(...sizes)).toBeLessThanOrEqual(1);
              expect(Math.max(...sizes)).toBeLessThanOrEqual(4);
            }
            if (alias) {
              // Privacy: no account name appears anywhere, accessible names included.
              expect(seen.html).not.toContain(NICKNAME_MARK);
            } else if (surface.window === "overview" && count > 0) {
              expect(seen.html).toContain(NICKNAME_MARK);
            }
            await host.screenshot(`matrix/counts/${label}`, canvas(page, surface));
          });
        }
      }
    }
  }
});

/** What the full window's badge says for each state. */
const BADGE: Record<AccountState, string> = {
  healthy: "Current",
  low: "5h low",
  "rate limited": "Rate limited",
  offline: "Offline",
  "check failed": "Check failed",
  "reconnect needed": "Reconnect",
};

test.describe("states", () => {
  for (const surface of SURFACES) {
    for (const state of STATES) {
      for (const theme of THEMES) {
        test(`${surface.name}: ${state}, ${theme}`, async ({ open }) => {
          const accounts = [accountIn(state, "claude", 1)];
          const host = await open(
            configFor(surface, accounts, theme, false),
            HASH[surface.window],
          );
          const { page } = host;
          if (surface.window === "overview") {
            await expect(page.locator(".provider-card .badge")).toHaveText(BADGE[state]);
          } else if (surface.style === "ring") {
            // Under the rings the strip writes every ring's value. A passing
            // problem (rate limited, offline, check failed) keeps the last
            // readings, muted, as the full window does; only an account that
            // must be reconnected writes its state instead.
            const tile = page.locator(".widget-tile");
            if (state === "healthy") {
              await expect(tile.locator(".widget-readings")).toContainText("82%");
              await expect(tile.locator(".widget-readings")).toContainText("61%");
              await expect(tile.locator(".widget-value.low")).toHaveCount(0);
            } else if (state === "low") {
              await expect(
                tile.locator(".widget-readings .widget-value.low"),
              ).toContainText("8%");
            } else if (state === "reconnect needed") {
              await expect(tile.locator(".widget-headline")).toContainText(BADGE[state]);
            } else {
              await expect(tile.locator(".widget-readings.last-known")).toBeVisible();
              await expect(tile.locator(".widget-headline")).toHaveCount(0);
            }
          } else {
            // The mini cards draw readings, not statuses: only an account with no
            // reading at all (reconnect needed) says its state.
            await expect(page.locator(".widget-card")).toHaveCount(1);
            if (state === "reconnect needed") {
              await expect(page.locator(".widget-card")).toContainText("Reconnect");
            } else {
              await expect(
                page.locator(".widget-card .widget-bar").first(),
              ).toBeVisible();
            }
          }
          await host.screenshot(
            `matrix/states/${surface.id}-${state.replaceAll(" ", "-")}-${theme}`,
            canvas(page, surface),
          );
        });
      }
    }
  }
});

/** Two accounts of one provider with names far too long for their room. */
function longNamed(): AccountSnapshot[] {
  const long =
    "Extraordinarily long nickname that someone typed into the rename box without thinking about the layout";
  const mk = (id: string, name: string, remaining: number): AccountSnapshot =>
    account(
      id,
      "claude",
      id === "long-1" ? 1 : 2,
      [
        quotaWindow(`${id}-5h`, "session", percent(remaining), { label: "5-hour" }),
        quotaWindow(`${id}-wk`, "weekly", percent(remaining), { label: "Weekly" }),
      ],
      { nickname: name, rank: remaining },
    );
  return [mk("long-1", `${long} one`, 70), mk("long-2", `${long} two`, 20)];
}

test.describe("long names", () => {
  for (const surface of SURFACES) {
    for (const theme of THEMES) {
      test(`${surface.name}: two accounts with very long names, ${theme}`, async ({
        open,
      }) => {
        const host = await open(
          configFor(surface, longNamed(), theme, false),
          HASH[surface.window],
        );
        const { page } = host;
        await expect(
          page.locator(".provider-card, .widget-tile, .widget-card").first(),
        ).toBeVisible();
        const seen = await shown(page, surface);
        // Nothing may push the window wider than it is.
        expect(seen.scrollWidth).toBeLessThanOrEqual(seen.innerWidth);
        if (surface.window === "widget") expect(seen.widget).toBe(WIDGET_WIDTH);
        await host.screenshot(
          `matrix/long-names/${surface.id}-${theme}`,
          canvas(page, surface),
        );
      });
    }
  }

  test("with aliases on the long names are not shown at all", async ({ open }) => {
    for (const surface of SURFACES) {
      const host = await open(
        configFor(surface, longNamed(), "dark", true),
        HASH[surface.window],
      );
      const seen = await shown(host.page, surface);
      expect(seen.html).not.toContain("Extraordinarily");
      await host.page.goto("about:blank");
    }
  });
});

test.describe("widget sizes", () => {
  const COUNT_STEPS = [0, 1, 2, 3, 4, 5, 7, 12];

  for (const surface of SURFACES.filter((entry) => entry.window === "widget")) {
    test(`${surface.name}: always ${String(WIDGET_WIDTH)} px wide, and tall exactly as its accounts need`, async ({
      open,
    }) => {
      // The widget fits its content: it asks the host to fit the window to
      // exactly the height it measures, through the fit_widget command. The
      // code sets no minimum or maximum height; the width is fixed.
      const heights: number[] = [];
      for (const count of COUNT_STEPS) {
        const accounts = Array.from({ length: count }, (_, index) =>
          accountIn(
            STATES[index % STATES.length] ?? "healthy",
            "claude",
            index + 1,
            `${NICKNAME_MARK}-${String(index)}`,
          ),
        );
        // Twelve accounts of one provider need their names to be told apart.
        const host = await open(configFor(surface, accounts, "dark", false), HASH.widget);
        await expect(host.page.locator(".widget")).toBeVisible();
        const box = await host.page.locator(".widget").boundingBox();
        expect(box?.width).toBe(WIDGET_WIDTH);
        heights.push(box?.height ?? 0);
        await expect
          .poll(async () => {
            const sizes = await host.callsTo("fit_widget");
            const last = sizes.at(-1)?.args as
              { contentHeight: number; direction: string } | undefined;
            return last === undefined ? null : [WIDGET_WIDTH, last.contentHeight];
          })
          .toEqual([WIDGET_WIDTH, Math.ceil(box?.height ?? 0)]);
        await host.screenshot(
          `matrix/widget-sizes/${surface.id}-n${String(count)}`,
          host.page.locator(".widget"),
        );
        await host.page.goto("about:blank");
      }
      // More accounts never make the window shorter, and 12 is taller than 1.
      for (let index = 1; index < heights.length; index += 1) {
        expect(heights[index]).toBeGreaterThanOrEqual(heights[index - 1] ?? 0);
      }
      expect(heights.at(-1)).toBeGreaterThan(heights[1] ?? Infinity);
    });
  }

  test("the full window asks the host to fit its height, growing with its accounts", async ({
    open,
  }) => {
    const wanted: number[] = [];
    for (const count of [1, 2, 3, 7]) {
      const host = await open(
        configFor(SURFACES[0] as Surface, mixedAccounts(count), "dark", false),
        "",
      );
      await expect
        .poll(async () => (await host.callsTo("fit_overview_height")).length)
        .toBeGreaterThan(0);
      const call = (await host.callsTo("fit_overview_height")).at(-1);
      wanted.push((call?.args as { contentHeight: number }).contentHeight);
      await host.page.goto("about:blank");
    }
    for (let index = 1; index < wanted.length; index += 1) {
      expect(wanted[index]).toBeGreaterThan(wanted[index - 1] ?? 0);
    }
  });
});

test.describe("switching modes", () => {
  test("full window to widget and back: each side asks the host, and Expand returns", async ({
    open,
    secondWindow,
  }) => {
    const accounts = mixedAccounts(3);
    const popover = await open(
      configFor(SURFACES[0] as Surface, accounts, "dark", false),
      "",
    );
    const widget = await open(
      configFor(SURFACES[4] as Surface, accounts, "dark", false),
      "#/widget",
      await secondWindow(),
    );

    // Full window -> widget: the header button asks for the widget view.
    await popover.page.getByRole("button", { name: "Switch to the mini widget" }).click();
    expect((await popover.callsTo("set_app_view")).at(-1)?.args).toEqual({
      view: "widget",
    });
    // The host answers with the saved preference; it shows the widget window.
    const { preferences } = await popover.hostState();
    expect(preferences.view).toBe("widget");
    await widget.emit("preferences-changed", {
      app_instance_id: "ui-test-instance",
      preference_revision: preferences.revision,
      preferences,
    });
    await widget.page.bringToFront();
    await expect(widget.page.locator(".widget-tile")).toHaveCount(3);

    // Widget -> full window: Expand asks for the overview view, and nothing else.
    const before = (await widget.calls()).length;
    await widget.page.getByRole("button", { name: "Open the full window" }).click();
    const after = (await widget.calls())
      .slice(before)
      .filter((call) => call.command !== "fit_widget");
    expect(after.map((call) => [call.command, call.args])).toEqual([
      ["set_app_view", { view: "overview" }],
    ]);
    expect((await widget.hostState()).preferences.view).toBe("overview");
    await popover.page.bringToFront();
    await expect(popover.page.locator(".provider-card")).toHaveCount(3);
  });

  test("the Settings switch Mini widget asks for each view", async ({ open }) => {
    const host = await open(scenario("settings"), "#/settings/general");
    const toggle = host.page.getByRole("switch", { name: "Mini widget" });
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    expect((await host.callsTo("set_app_view")).map((call) => call.args)).toEqual([
      { view: "widget" },
      { view: "overview" },
    ]);
  });

  test("the pin switches the full window between docked and floating", async ({
    open,
  }) => {
    const host = await open(
      configFor(SURFACES[0] as Surface, mixedAccounts(2), "dark", false),
      "",
    );
    const { page } = host;
    await expect(page.locator(".pin-label")).toHaveCount(0);
    await page.getByRole("button", { name: "Float as a separate window" }).click();
    await expect(page.locator(".popover.pinned")).toHaveCount(1);
    await expect(page.locator(".pin-label")).toHaveText("Floating");
    await host.screenshot(
      "matrix/switching/full-window-floating",
      page.locator(".popover"),
    );
    await page.getByRole("button", { name: "Dock to the tray" }).click();
    await expect(page.locator(".popover.pinned")).toHaveCount(0);
    expect((await host.callsTo("set_overview_mode")).map((call) => call.args)).toEqual([
      { mode: "floating" },
      { mode: "tray" },
    ]);
  });

  test("the layout buttons switch rings and bars in the full window and the widget look", async ({
    open,
    secondWindow,
  }) => {
    const host = await open(
      configFor(SURFACES[0] as Surface, mixedAccounts(3), "dark", false),
      "",
    );
    await host.page.getByRole("button", { name: "Compact layout" }).click();
    expect((await host.callsTo("set_indicator_style")).at(-1)?.args).toEqual({
      style: "bar",
    });
    await expect(
      host.page.getByRole("button", { name: "Compact layout" }),
    ).toHaveAttribute("aria-pressed", "true");
    // The same preference decides the widget's look.
    const widget = await open(
      configFor(SURFACES[5] as Surface, mixedAccounts(3), "dark", false),
      "#/widget",
      await secondWindow(),
    );
    await expect(widget.page.locator(".widget-card")).toHaveCount(3);
    await expect(widget.page.locator(".widget-tile")).toHaveCount(0);
  });

  test("a widget account with monitoring off is left out, and the rest remain", async ({
    open,
  }) => {
    const accounts = [
      ...mixedAccounts(2),
      { ...accountIn("healthy", "grok", 3), monitoring_enabled: false },
    ];
    const host = await open(
      configFor(SURFACES[4] as Surface, accounts, "dark", false),
      "#/widget",
    );
    await expect(host.page.locator(".widget-tile")).toHaveCount(2);
  });
});
