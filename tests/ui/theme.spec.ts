/**
 * Theme: a chosen theme is applied, "system" follows the operating system and
 * reacts when it changes, and the choice is saved through the host.
 */
import type { Page } from "@playwright/test";

import { expect, test } from "./harness";
import { defaultPreferences, scenario } from "./scenarios";

const theme = (page: Page) =>
  page.evaluate(() => document.documentElement.dataset["theme"]);

test.describe("theme", () => {
  test("a dark preference paints dark and a light one paints light", async ({ open }) => {
    const dark = await open(
      scenario("overview", { preferences: defaultPreferences({ theme: "dark" }) }),
    );
    await expect.poll(() => theme(dark.page)).toBe("dark");
  });

  test("a light preference paints light even when the system is dark", async ({
    open,
    page,
  }) => {
    await page.emulateMedia({ colorScheme: "dark" });
    const host = await open(
      scenario("overview", { preferences: defaultPreferences({ theme: "light" }) }),
    );
    await expect.poll(() => theme(host.page)).toBe("light");
    await host.screenshot("theme-light-on-dark-system");
  });

  test("system follows the operating system and its changes", async ({ open, page }) => {
    await page.emulateMedia({ colorScheme: "dark" });
    const host = await open(
      scenario("overview", { preferences: defaultPreferences({ theme: "system" }) }),
    );
    await expect.poll(() => theme(host.page)).toBe("dark");
    await page.emulateMedia({ colorScheme: "light" });
    await expect.poll(() => theme(host.page)).toBe("light");
  });

  test("choosing a theme in settings saves it and repaints", async ({ open }) => {
    const host = await open(
      scenario("settings", { preferences: defaultPreferences({ theme: "dark" }) }),
      "#/settings/appearance",
    );
    const { page } = host;
    await page.getByRole("button", { name: "Light", exact: true }).click();
    await expect.poll(() => theme(page)).toBe("light");
    const saves = await host.callsTo("update_preferences");
    expect(
      (saves.at(-1)?.args as { preferences: { theme: string } }).preferences.theme,
    ).toBe("light");
    await host.screenshot("settings-appearance-light");
    await page.getByRole("button", { name: "Dark", exact: true }).click();
    await expect.poll(() => theme(page)).toBe("dark");
    await host.screenshot("settings-appearance-dark");
  });
});
