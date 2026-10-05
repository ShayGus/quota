/**
 * The update pop-up, rendered by the real renderer against the faked host.
 *
 * The host owns the update flow and decides what the pop-up shows; here the
 * faked host plays that part, so these tests prove what the window draws and
 * which answer each button sends. The relaunch itself belongs to the host and is
 * recorded by the faked host as a `(host) relaunch` call. Screenshots in both
 * themes are saved beside the settings window's, to compare the two.
 */
import type { Page } from "@playwright/test";

import type { UpdatePrompt } from "../../src/generated/bindings";
import { expect, test } from "./harness";
import { defaultPreferences, scenario } from "./scenarios";

const OFFER: UpdatePrompt = {
  kind: "offer",
  context: { version: "0.2.0", current: "0.1.0" },
};

const OFFER_TEXT =
  "Quota 0.2.0 is available (you have 0.1.0). Install it now? Quota will restart.";

const FAILURE_TEXT =
  "The update could not be installed. Quota will keep running the current version.";

/** A pop-up window whose host has an update to offer. */
function popup(
  theme: "light" | "dark" = "light",
  installOutcome: "restarts" | "fails" = "restarts",
) {
  return scenario("update", {
    preferences: defaultPreferences({ theme }),
    update: { prompt: OFFER, installOutcome },
  });
}

/** Answers the host has been sent, in order. */
async function answers(host: {
  callsTo: (command: string) => Promise<{ args: unknown }[]>;
}): Promise<string[]> {
  return (await host.callsTo("respond_to_update_prompt")).map(
    (call) => (call.args as { response: string }).response,
  );
}

const button = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true });

test.describe("the update pop-up", () => {
  test("an available update names both versions and offers OK and Cancel", async ({
    open,
  }) => {
    const host = await open(popup(), "#/update");
    const { page } = host;
    await expect(page.getByRole("heading", { name: "Update available" })).toBeVisible();
    await expect(page.getByText(OFFER_TEXT)).toBeVisible();
    await expect(button(page, "OK")).toBeEnabled();
    await expect(button(page, "Cancel")).toBeEnabled();
    // Enter answers OK: it holds the focus.
    await expect(button(page, "OK")).toBeFocused();
    expect(await answers(host)).toEqual([]);
    await host.screenshot("update-popup/offer-light");
    await host.screenshotFull("update-offer");
  });

  test("it is drawn in the dark theme too", async ({ open }) => {
    const host = await open(popup("dark"), "#/update");
    await expect
      .poll(() => host.page.evaluate(() => document.documentElement.dataset["theme"]))
      .toBe("dark");
    await expect(host.page.getByText(OFFER_TEXT)).toBeVisible();
    await host.screenshot("update-popup/offer-dark");
  });

  test("OK sends the install answer, shows the busy state, and the host relaunches", async ({
    open,
  }) => {
    const host = await open(popup(), "#/update");
    const { page } = host;
    await button(page, "OK").click();
    expect(await answers(host)).toEqual(["install"]);

    // Busy: the same window, stating the install, with nothing to press.
    await expect(
      page.getByRole("heading", { name: "Installing the update" }),
    ).toBeVisible();
    await expect(page.getByText(/Installing Quota 0\.2\.0/)).toBeVisible();
    await expect(button(page, "OK")).toBeDisabled();
    await expect(button(page, "Cancel")).toBeDisabled();
    await expect(button(page, "Close update window")).toBeDisabled();
    await host.screenshot("update-popup/installing-light");
    // Escape does not cancel an install that has started.
    await page.keyboard.press("Escape");
    expect(await answers(host)).toEqual(["install"]);

    await host.finishUpdate();
    await expect
      .poll(async () => (await host.calls()).map((call) => call.command))
      .toContain("(host) relaunch");
    expect(await answers(host)).toEqual(["install"]);
  });

  test("Cancel sends the decline answer and nothing is installed", async ({ open }) => {
    const host = await open(popup(), "#/update");
    await button(host.page, "Cancel").click();
    expect(await answers(host)).toEqual(["decline"]);
    const commands = (await host.calls()).map((call) => call.command);
    expect(commands).toContain("(host) close the pop-up");
    expect(commands).not.toContain("(host) relaunch");
  });

  test("the header's close button and Escape both mean Cancel", async ({
    open,
    secondWindow,
  }) => {
    const first = await open(popup(), "#/update");
    await first.page.getByRole("button", { name: "Close update window" }).click();
    expect(await answers(first)).toEqual(["decline"]);

    const second = await open(popup(), "#/update", await secondWindow());
    await expect(second.page.getByText(OFFER_TEXT)).toBeVisible();
    await second.page.keyboard.press("Escape");
    await expect.poll(() => answers(second)).toEqual(["decline"]);
  });

  test("a failed install says so in the same window, with one button to close it", async ({
    open,
  }) => {
    const host = await open(popup("light", "fails"), "#/update");
    const { page } = host;
    await button(page, "OK").click();
    await expect(page.getByText(/Installing Quota/)).toBeVisible();
    await host.finishUpdate();

    await expect(page.getByRole("heading", { name: "Update failed" })).toBeVisible();
    await expect(page.getByText(FAILURE_TEXT)).toBeVisible();
    await expect(button(page, "Close")).toBeEnabled();
    await expect(button(page, "OK")).toHaveCount(0);
    await expect(button(page, "Cancel")).toHaveCount(0);
    await host.screenshot("update-popup/failed-light");

    await button(page, "Close").click();
    expect(await answers(host)).toEqual(["install", "dismiss"]);
    expect((await host.calls()).map((call) => call.command)).not.toContain(
      "(host) relaunch",
    );
  });

  test("a window that opens after the host spoke still shows the offer", async ({
    open,
  }) => {
    // The host decides what to show before this window has loaded, so the window
    // asks once on opening instead of waiting for an event that already went by.
    const host = await open(popup(), "#/update");
    await expect(host.page.getByText(OFFER_TEXT)).toBeVisible();
    expect(await host.callsTo("get_update_prompt")).toHaveLength(1);
  });

  test("it looks like the settings window: same header, text, buttons and themes", async ({
    open,
    secondWindow,
  }) => {
    for (const theme of ["light", "dark"] as const) {
      // Compare the settled design, rather than sampling a theme transition.
      const preferences = defaultPreferences({ theme, reduce_motion: true });
      const update = await open(
        scenario("update", {
          preferences,
          update: { prompt: OFFER, installOutcome: "restarts" },
        }),
        "#/update",
        await secondWindow(),
      );
      const settings = await open(
        scenario("settings", { preferences }),
        "#/settings/accounts",
        await secondWindow(),
      );
      await expect(update.page.getByText(OFFER_TEXT)).toBeVisible();
      await expect(
        settings.page.getByRole("heading", { name: "Quota settings" }),
      ).toBeVisible();
      for (const { page } of [update, settings]) {
        await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
        await expect(page.locator("html")).toHaveAttribute("data-reduce-motion", "true");
      }

      const style = (page: Page, selector: string) =>
        page.evaluate((query) => {
          const element = document.querySelector(query);
          if (element === null) return null;
          const computed = getComputedStyle(element);
          return {
            font: `${computed.fontFamily}|${computed.fontSize}|${computed.fontWeight}`,
            color: computed.color,
            background: computed.backgroundColor,
            border: computed.borderBottomColor + computed.borderBottomWidth,
            padding: computed.padding,
          };
        }, selector);

      // The header, its title, its subtitle and the window itself are the
      // settings window's own, down to computed style.
      for (const selector of [
        ".window",
        ".settings-head",
        ".settings-head h2",
        ".settings-head p",
        ".settings-head .icon-btn",
        ".button.primary",
      ]) {
        const here = await style(update.page, selector);
        const there = await style(settings.page, selector);
        expect(here, `${selector} in ${theme}`).not.toBeNull();
        expect(here, `${selector} in ${theme}`).toEqual(there);
      }
      // The text is the dialog's text.
      const dialogText = await style(update.page, ".update-body p");
      expect(dialogText?.font.endsWith("|12px|400")).toBe(true);

      await update.screenshot(`update-popup/compare-${theme}-update`);
      await settings.screenshot(`update-popup/compare-${theme}-settings`);
    }
  });
});
