/**
 * Report a bug: the header's menu, the one beside the widget's expand button,
 * and the section in Settings → Diagnostics. Each asks the host, which builds
 * the issue address and the prompt itself; the renderer sends no arguments.
 */
import { expect, test } from "./harness";
import { defaultPreferences, scenario } from "./scenarios";

const ISSUE = "Open an issue on GitHub";
const PROMPT = "Copy a prompt for an AI agent";

test.describe("report a bug", () => {
  for (const theme of ["light", "dark"] as const) {
    test(`the header offers both choices, ${theme}`, async ({ open }) => {
      const host = await open(
        scenario("overview", { preferences: defaultPreferences({ theme }) }),
      );
      const { page } = host;
      await page.getByRole("button", { name: "Report a bug" }).click();
      const menu = page.getByRole("menu", { name: "Report a bug" });
      await expect(menu.getByRole("menuitem")).toHaveText([ISSUE, PROMPT]);
      await host.screenshot(`report-bug/header-menu-${theme}`);
      if (theme === "dark") {
        await host.screenshot("docs/report-bug-menu");
      }
    });
  }

  test("the header opens the issue form through the host", async ({ open }) => {
    const host = await open(scenario("overview"));
    const { page } = host;
    await page.getByRole("button", { name: "Report a bug" }).click();
    await page.getByRole("menuitem", { name: ISSUE }).click();
    await expect
      .poll(async () => (await host.callsTo("open_bug_report_issue")).length)
      .toBe(1);
    expect((await host.callsTo("open_bug_report_issue"))[0]?.args ?? {}).toEqual({});
    await expect(page.getByRole("menu")).toHaveCount(0);
  });

  test("the header copies the prompt and confirms it", async ({ open }) => {
    const host = await open(scenario("overview"));
    const { page } = host;
    await page.getByRole("button", { name: "Report a bug" }).click();
    await page.getByRole("menuitem", { name: PROMPT }).click();
    await expect
      .poll(async () => (await host.callsTo("copy_bug_report_prompt")).length)
      .toBe(1);
    await expect(page.getByText(/Prompt copied/)).toBeVisible();
    await host.screenshot("report-bug/header-copied");
  });

  test("Escape closes the menu and keeps the window", async ({ open }) => {
    const host = await open(scenario("overview"));
    const { page } = host;
    await page.getByRole("button", { name: "Report a bug" }).click();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("menu")).toHaveCount(0);
    await expect(page.getByRole("article").first()).toBeVisible();
  });

  test("the widget offers both choices beside its expand button", async ({ open }) => {
    const host = await open(
      scenario("widget", { preferences: defaultPreferences({ view: "widget" }) }),
      "#/widget",
    );
    const { page } = host;
    await page.locator(".widget").hover();
    await page.getByRole("button", { name: "Report a bug" }).click();
    await expect(page.getByRole("menuitem")).toHaveText([ISSUE, PROMPT]);
    // The host grows the window to the widget's content; the page stands in.
    const box = await page.locator(".widget").boundingBox();
    const height = Math.ceil(box?.height ?? 0);
    expect(height).toBeGreaterThan(172);
    await page.setViewportSize({ width: 316, height });
    await host.screenshot("report-bug/widget-menu");
    await host.screenshot("docs/widget-report-bug");
    await page.getByRole("menuitem", { name: PROMPT }).click();
    await expect
      .poll(async () => (await host.callsTo("copy_bug_report_prompt")).length)
      .toBe(1);
    await expect(page.getByRole("status")).toContainText("Prompt copied");
    await host.screenshot("report-bug/widget-copied");
  });

  test("Settings → Diagnostics offers both choices", async ({ open }) => {
    const host = await open(scenario("settings"), "#/settings/diagnostics");
    const { page } = host;
    await expect(page.getByRole("heading", { name: "Report a bug" })).toBeVisible();
    await page.getByRole("button", { name: ISSUE }).click();
    await expect
      .poll(async () => (await host.callsTo("open_bug_report_issue")).length)
      .toBe(1);
    await page.getByRole("button", { name: PROMPT }).click();
    await expect
      .poll(async () => (await host.callsTo("copy_bug_report_prompt")).length)
      .toBe(1);
    await expect(
      page.getByRole("status").filter({ hasText: "Prompt copied" }),
    ).toBeVisible();
    await host.screenshot("report-bug/settings-diagnostics");
  });
});
