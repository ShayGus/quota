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
  });

  test("with no accounts it says so", async ({ open }) => {
    const host = await open(scenario("widget", { accounts: [] }), "#/widget");
    await expect(host.page.getByText("No accounts to show")).toBeVisible();
  });
});
