/**
 * A prepaid balance in the real renderer: the OpenRouter card, its detail, and
 * the switch that shows the key's spend limit.
 */
import { expect, test } from "./harness";
import { defaultPreferences, openRouterAccount, scenario } from "./scenarios";

test.describe("prepaid balance", () => {
  test("the card shows the money left of the last top-up and how long it lasts", async ({
    open,
  }) => {
    const host = await open(scenario("overview", { accounts: [openRouterAccount()] }));
    const card = host.page.getByRole("article");
    await expect(card).toContainText("$37.20");
    await expect(card).toContainText("of $50.00 loaded on Sep 28");
    await expect(card).toContainText("≈ 12 days at $3.10/day");
    // The key's limit is hidden until the person shows it.
    await expect(card).not.toContainText("API key limit");
    await host.screenshot("openrouter-card");
    await host.screenshotFull("openrouter-card");
  });

  test("a shown key limit sits beside the balance", async ({ open }) => {
    const host = await open(
      scenario("overview", { accounts: [openRouterAccount({ showKeyLimit: true })] }),
    );
    const card = host.page.getByRole("article");
    await expect(card.locator(".quota-button")).toHaveCount(2);
    await expect(card).toContainText("API key limit");
    await host.screenshot("openrouter-card-key-limit");
  });

  test("the compact layout shows the pace instead of a reset", async ({ open }) => {
    const host = await open(
      scenario("overview", {
        accounts: [openRouterAccount()],
        preferences: defaultPreferences({ indicator_style: "bar" }),
      }),
    );
    await expect(host.page.getByRole("article")).toContainText("≈ 12 d");
    await host.screenshot("openrouter-card-bar");
  });

  test("the detail shows the share, the spending and the top-ups", async ({ open }) => {
    const host = await open(scenario("overview", { accounts: [openRouterAccount()] }));
    const { page } = host;
    await page
      .getByRole("button", { name: /Details for OpenRouter Side project/ })
      .click();
    await expect(page.getByText("LEFT OF LAST TOP-UP")).toBeVisible();
    await expect(page.getByText("74% left")).toBeVisible();
    await expect(page.getByText("$12.80 spent / $37.20 left of $50.00")).toBeVisible();
    await expect(
      page.getByRole("list", { name: "Top-ups" }).getByRole("listitem"),
    ).toHaveCount(2);
    await host.screenshot("openrouter-detail");
    await host.screenshotFull("openrouter-detail");
  });

  test("the key-limit switch asks the host and shows what it confirms", async ({
    open,
  }) => {
    const host = await open(
      scenario("settings", { accounts: [openRouterAccount()] }),
      "#/settings/accounts",
    );
    const toggle = host.page.getByRole("switch", {
      name: "Show the key spend limit of OpenRouter Side project",
    });
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await host.screenshotFull("settings-accounts-openrouter");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    expect((await host.callsTo("set_key_limit_shown")).at(-1)?.args).toEqual({
      accountRef: { id: "acct-openrouter" },
      shown: true,
    });
  });
});
