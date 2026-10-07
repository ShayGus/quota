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

  test("the compact bar layout renders", async ({ open }) => {
    const host = await open(
      scenario("widget", {
        preferences: defaultPreferences({ view: "widget", indicator_style: "bar" }),
      }),
      "#/widget",
    );
    await expect(
      host.page.getByRole("button", { name: "Open the full window" }),
    ).toBeVisible();
    await host.screenshot("widget-bars");
    await host.screenshotFull("widget-cards");
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

  test("captures a press that moves below the widget while the drawer unfolds", async ({
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
      element.getAnimations().forEach((animation) => animation.pause());
    });
    const bounds = await page.locator(".widget").boundingBox();
    if (bounds === null) throw new Error("the widget is not visible");
    const x = bounds.x + bounds.width / 2;
    const y = bounds.y + bounds.height - 1;
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
});
