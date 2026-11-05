/**
 * What's new: the panel the first run of a new version opens.
 */
import { expect, test } from "./harness";
import { scenario } from "./scenarios";

test.describe("what's new", () => {
  test("lists what changed in this version", async ({ open }) => {
    const host = await open(scenario("release-notes"), "#/release-notes");
    const { page } = host;
    await expect(page.getByRole("heading", { name: "What's new" })).toBeVisible();
    await expect(page.getByText("Quota 0.1.11")).toBeVisible();
    await expect(page.getByRole("listitem")).toHaveCount(2);
    await host.screenshot("release-notes");
    await host.screenshotFull("release-notes");
  });

  test("Close has the focus, so Enter closes it", async ({ open }) => {
    const host = await open(scenario("release-notes"), "#/release-notes");
    await expect(
      host.page.getByRole("button", { name: "Close", exact: true }),
    ).toBeFocused();
  });

  for (const [name, leave] of [
    ["its button", "button"],
    ["Esc", "escape"],
  ] as const) {
    test(`closes with ${name}`, async ({ open }) => {
      const host = await open(scenario("release-notes"), "#/release-notes");
      const { page } = host;
      await expect(page.getByRole("listitem").first()).toBeVisible();
      if (leave === "button") {
        await page.getByRole("button", { name: "Close", exact: true }).click();
      } else {
        await page.keyboard.press("Escape");
      }
      await expect
        .poll(async () => (await host.callsTo("plugin:window|close")).length)
        .toBe(1);
    });
  }
});
