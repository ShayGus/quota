/**
 * The popover is a 440-pixel-wide window. Whatever the accounts show, the
 * content must fit that width without a sideways scrollbar, in both themes and
 * both layouts.
 */
import { expect, test, WINDOW_SIZE } from "./harness";
import { defaultPreferences, scenario } from "./scenarios";

const LAYOUTS = ["ring", "bar"] as const;
const THEMES = ["light", "dark"] as const;

test.describe("popover at 440 px", () => {
  for (const layout of LAYOUTS) {
    for (const theme of THEMES) {
      test(`fits its width: ${layout} layout, ${theme} theme`, async ({ open }) => {
        const host = await open(
          scenario("overview", {
            preferences: defaultPreferences({ indicator_style: layout, theme }),
          }),
        );
        const { page } = host;
        await expect(page.getByRole("article").first()).toBeVisible();
        expect(WINDOW_SIZE.overview.width).toBe(440);
        const metrics = await page.evaluate(() => ({
          viewport: window.innerWidth,
          scrollWidth: document.documentElement.scrollWidth,
          popover: document.querySelector(".popover")?.getBoundingClientRect().width,
        }));
        expect(metrics.viewport).toBe(440);
        expect(metrics.scrollWidth).toBeLessThanOrEqual(440);
        expect(metrics.popover).toBe(440);
        await host.screenshot(`popover-440-${layout}-${theme}`);
        await host.screenshotFull(`overview-${layout}-${theme}`);
      });
    }
  }

  test("asks the host to fit its height to the content", async ({ open }) => {
    const host = await open(scenario("overview"));
    await expect
      .poll(async () => (await host.callsTo("fit_overview_height")).length)
      .toBeGreaterThan(0);
    const [call] = await host.callsTo("fit_overview_height");
    const height = (call?.args as { contentHeight: number }).contentHeight;
    expect(height).toBeGreaterThan(0);
  });
});
