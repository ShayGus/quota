/**
 * The mini widget window: the compact view, in both layouts, and its way back
 * to the full window.
 */
import { expect, test } from "./harness";
import { defaultPreferences, scenario } from "./scenarios";

test.describe("widget", () => {
  test("shows the rings and expands to the full window", async ({ open }) => {
    const host = await open(
      scenario("widget", { preferences: defaultPreferences({ view: "widget" }) }),
      "#/widget",
    );
    const { page } = host;
    await expect(
      page.getByRole("button", { name: "Open the full window" }),
    ).toBeVisible();
    await host.screenshot("widget-rings");
    await host.screenshotFull("widget-rings");
    await page.getByRole("button", { name: "Open the full window" }).click();
    expect((await host.callsTo("set_app_view")).at(-1)?.args).toEqual({
      view: "overview",
    });
  });

  const LIST = scenario("widget", {
    preferences: defaultPreferences({ view: "widget", indicator_style: "bar" }),
  });

  test("the compact list renders one row per account, at its own width", async ({
    open,
  }) => {
    const host = await open(LIST, "#/widget");
    const { page } = host;
    const rows = page.locator(".widget-row");
    await expect(rows.first()).toBeVisible();
    const box = await page.locator(".widget").boundingBox();
    expect(box?.width ?? 0).toBeLessThanOrEqual(400);
    await expect
      .poll(async () => (await host.callsTo("fit_widget")).at(-1)?.args)
      .toMatchObject({ contentWidth: Math.ceil(box?.width ?? 0) });
    await host.screenshot("widget-bars");
    await host.screenshotFull("widget-list");
  });

  test("every row's limits start at the same point, in columns of one width", async ({
    open,
  }) => {
    const host = await open(LIST, "#/widget");
    const { page } = host;
    await expect(page.locator(".widget-row").first()).toBeVisible();
    const rows = await page.locator(".widget-row-limits").evaluateAll((all) =>
      all.map((limits) =>
        [...limits.querySelectorAll(".widget-limit")].map((limit) => {
          const box = limit.getBoundingClientRect();
          return { left: box.left, width: box.width };
        }),
      ),
    );
    const filled = rows.filter((row) => row.length > 0);
    expect(filled.length).toBeGreaterThan(1);
    const firsts = filled.map((row) => row[0]?.left ?? 0);
    expect(Math.max(...firsts) - Math.min(...firsts)).toBeLessThanOrEqual(0.5);
    const widths = filled.flat().map((limit) => limit.width);
    expect(Math.max(...widths) - Math.min(...widths)).toBeLessThanOrEqual(0.5);
    const second = filled.map((row) => row[1]?.left).filter((left) => left !== undefined);
    expect(Math.max(...second) - Math.min(...second)).toBeLessThanOrEqual(0.5);
  });

  test("every row has its provider's icon, and words end where numbers end", async ({
    open,
  }) => {
    const host = await open(LIST, "#/widget");
    const { page } = host;
    const rows = page.locator(".widget-row");
    await expect(rows.first()).toBeVisible();
    await expect(page.locator(".widget-row .provider-icon")).toHaveCount(
      await rows.count(),
    );
    const edges = await page.evaluate(() => {
      const rightOf = (selector: string): number[] =>
        [...document.querySelectorAll(selector)].map(
          (element) => element.getBoundingClientRect().right,
        );
      return {
        words: rightOf(".widget-row-status"),
        limits: rightOf(".widget-limit .widget-value"),
      };
    });
    expect(edges.words.length).toBeGreaterThan(0);
    const end = Math.max(...edges.limits);
    for (const right of edges.words) {
      expect(Math.abs(right - end)).toBeLessThanOrEqual(0.5);
    }
  });

  test("the list is see-through until pointed at, and its rim stays", async ({
    open,
  }) => {
    const host = await open(LIST, "#/widget");
    const { page } = host;
    const list = page.locator(".widget-list");
    const fill = (): Promise<string> =>
      list.evaluate((element) => getComputedStyle(element).backgroundColor);
    const rim = (): Promise<string> =>
      list.evaluate((element) => getComputedStyle(element).boxShadow);
    await expect.poll(fill).toContain("0.75");
    const before = await rim();
    expect(before).toContain("inset");
    await page.locator(".widget-row").first().hover();
    await expect(page.locator(".widget.solid")).toBeVisible();
    await expect.poll(fill).toMatch(/^rgb\(\d+, \d+, \d+\)$/);
    expect(await rim()).toBe(before);
    await host.screenshot("widget-list-solid");
  });

  test("a row opens its details, which never make the list wider", async ({ open }) => {
    const host = await open(LIST, "#/widget");
    const { page } = host;
    await page.setViewportSize({ width: 316, height: 400 });
    const row = page.locator(".widget-row").first();
    const before = await page.locator(".widget").boundingBox();
    await row.click();
    await expect(row).toHaveAttribute("aria-expanded", "true");
    const details = page.locator(".widget-details");
    await expect(details).toBeVisible();
    await expect(details.getByRole("button", { name: "Open full window" })).toBeVisible();
    const after = await page.locator(".widget").boundingBox();
    expect(after?.width).toBe(before?.width);
    expect(after?.height ?? 0).toBeGreaterThan(before?.height ?? 0);
    await host.screenshot("widget-list-details");
    await page.keyboard.press("Escape");
    await expect(details).toBeHidden();
    await expect(row).toBeFocused();
  });

  test("the details' Open full window switches to it", async ({ open }) => {
    const host = await open(LIST, "#/widget");
    const { page } = host;
    await page.setViewportSize({ width: 316, height: 400 });
    await page.locator(".widget-row").first().click();
    await page.getByRole("button", { name: "Open full window" }).click();
    expect((await host.callsTo("set_app_view")).at(-1)?.args).toEqual({
      view: "overview",
    });
  });

  test("the list has no report button: the tray has it", async ({ open }) => {
    const host = await open(LIST, "#/widget");
    await expect(host.page.locator(".widget-row").first()).toBeVisible();
    await expect(host.page.getByRole("button", { name: "Report a bug" })).toHaveCount(0);
  });

  test("Settings offers the shortcut that finds the widget", async ({ open }) => {
    const host = await open(scenario("settings"), "#/settings/general");
    const { page } = host;
    const change = page.locator(".find-shortcut .button").first();
    await expect(change).toHaveText("Ctrl+Alt+Q");
    await change.click();
    await expect(change).toHaveText("Press keys…");
    await page.keyboard.press("Control+Shift+F9");
    await expect
      .poll(async () => (await host.callsTo("set_find_widget_shortcut")).at(-1)?.args)
      .toEqual({ shortcut: "Ctrl+Shift+F9" });
    await host.screenshot("settings-find-shortcut");
  });

  test("Settings says when another app took the shortcut", async ({ open }) => {
    const host = await open(
      { ...scenario("settings"), shortcutActive: false },
      "#/settings/general",
    );
    await expect(host.page.getByRole("alert")).toHaveText(
      "Ctrl+Alt+Q is taken by another app. Pick another.",
    );
    await host.screenshot("settings-find-shortcut-taken");
  });

  test("Settings doesn't offer the shortcut where a desktop can't have it", async ({
    open,
  }) => {
    const host = await open(
      { ...scenario("settings"), shortcutSupported: false },
      "#/settings/general",
    );
    await expect(host.page.getByRole("switch", { name: "Mini widget" })).toBeVisible();
    await expect(host.page.getByText("Find the widget")).toHaveCount(0);
  });

  test("a hover only peeks, a click opens the drawer once", async ({ open }) => {
    const host = await open(
      scenario("widget", { preferences: defaultPreferences({ view: "widget" }) }),
      "#/widget",
    );
    const { page } = host;
    const tile = page.getByRole("button", { name: /^Codex/ });
    await tile.hover({ force: true });
    await expect(page.locator(".widget-foot.peeking")).toBeVisible();
    const drawer = page.getByRole("region", { name: "Codex" });
    await expect(drawer).toBeHidden();
    const sizes = await host.callsTo("fit_widget");
    await tile.click();
    await expect(drawer).toBeVisible();
    await expect(page.locator(".widget-strip .widget-drawer-close")).toBeVisible();
    const after = await host.callsTo("fit_widget");
    expect(after.length).toBe(sizes.length + 1);
    await host.screenshot("widget-drawer");
  });

  test("reconnects an account from its drawer, as the full window does", async ({
    open,
  }) => {
    const host = await open(
      scenario("widget", { preferences: defaultPreferences({ view: "widget" }) }),
      "#/widget",
    );
    const { page } = host;
    // Tall enough for the open drawer, which the faked host never sizes for.
    await page.setViewportSize({ width: 316, height: 360 });
    const tile = page.getByRole("button", {
      name: "Kimi needs reconnecting, open to reconnect",
    });
    await expect(tile).toContainText("Reconnect");
    await tile.click();
    const drawer = page.getByRole("region", { name: "Kimi" });
    await expect(drawer).toContainText("Reconnect to resume checks.");
    const button = drawer.getByRole("button", { name: "Reconnect Kimi" });
    await expect(button).toBeVisible();
    // Opening the drawer reconnects nothing; only its button does.
    expect(await host.callsTo("reconnect_account")).toEqual([]);
    await host.screenshot("widget-reconnect", page.locator(".widget"));
    await button.click();
    expect((await host.callsTo("reconnect_account")).at(-1)?.args).toEqual({
      accountRef: { id: "acct-kimi" },
    });
    await expect(drawer.locator(".widget-chip")).toHaveText("Connecting");
    await expect(
      drawer.getByRole("button", { name: "Reconnecting Kimi" }),
    ).toHaveAttribute("aria-disabled", "true");
    await host.screenshot("widget-reconnecting", page.locator(".widget"));
  });

  test("says in the drawer why a refused reconnect did not start", async ({ open }) => {
    const host = await open(
      scenario("widget", {
        preferences: defaultPreferences({ view: "widget" }),
        refuse: {
          reconnect_account: {
            kind: "persistence_unavailable",
            context: { owner: "accounts" },
          },
        },
      }),
      "#/widget",
    );
    const { page } = host;
    await page.setViewportSize({ width: 316, height: 360 });
    await page
      .getByRole("button", { name: "Kimi needs reconnecting, open to reconnect" })
      .click();
    const drawer = page.getByRole("region", { name: "Kimi" });
    await drawer.getByRole("button", { name: "Reconnect Kimi" }).click();
    await expect(drawer.getByRole("alert")).toHaveText(
      "Quota cannot reach its local storage.",
    );
    await expect(drawer.getByRole("button", { name: "Reconnect Kimi" })).toHaveAttribute(
      "aria-disabled",
      "false",
    );
    await host.screenshot("widget-reconnect-refused", page.locator(".widget"));
  });

  for (const start of ["edge", "empty area"] as const) {
    test(`tracks a press starting at the ${start} while the drawer unfolds`, async ({
      open,
    }) => {
      const host = await open(
        scenario("widget", { preferences: defaultPreferences({ view: "widget" }) }),
        "#/widget",
      );
      const { page } = host;
      await page.setViewportSize({ width: 316, height: 400 });
      await page.addStyleTag({ content: ".widget-drawer { transition-duration: 10s; }" });
      await page.getByRole("button", { name: /^Codex/ }).click();
      const drawer = page.getByRole("region", { name: "Codex" });
      await expect(drawer).toBeVisible();
      await drawer.evaluate((element) => {
        element.getAnimations().forEach((animation) => {
          animation.pause();
        });
      });
      const bounds = await page.locator(".widget").boundingBox();
      if (bounds === null) throw new Error("the widget is not visible");
      const x = bounds.x + bounds.width / 2;
      const y = bounds.y + bounds.height + (start === "edge" ? -1 : 3);
      if (start === "empty area") {
        expect(
          await page.evaluate(
            ({ x, y }) => document.elementFromPoint(x, y)?.closest(".widget") === null,
            { x, y },
          ),
        ).toBe(true);
      }
      await page.mouse.move(x, y);
      await page.mouse.down();
      await page.mouse.move(x, y + 4);
      expect(
        await page.evaluate(
          ({ x, y }) => document.elementFromPoint(x, y)?.closest(".widget") === null,
          { x, y: y + 4 },
        ),
      ).toBe(true);
      expect(await host.callsTo("plugin:window|start_dragging")).toHaveLength(1);
      await page.mouse.move(x, y + 20);
      expect(await host.callsTo("plugin:window|start_dragging")).toHaveLength(1);
      await page.mouse.up();
      await expect(drawer).toBeVisible();
      await page.getByRole("button", { name: "Open the full window" }).click();
      expect(await host.callsTo("set_app_view")).toHaveLength(1);
    });
  }
});
