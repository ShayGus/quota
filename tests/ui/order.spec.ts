/**
 * The account order in the real renderer: choosing it in Appearance,
 * arranging accounts with the arrows in Accounts, and the overview and the
 * widget following the arrangement.
 */
import { expect, test } from "./harness";
import { defaultPreferences, overviewAccounts, scenario } from "./scenarios";

/** The overview's accounts in the order of their ids, reversed. */
function reversedOrder(): string[] {
  return overviewAccounts()
    .map((account) => account.account_id)
    .reverse();
}

test.describe("account order", () => {
  test("choosing My order starts from the order on screen", async ({ open }) => {
    const host = await open(scenario("settings"), "#/settings/appearance");
    const { page } = host;
    const order = page.getByRole("group", { name: "Account order" });
    await expect(order.getByRole("button", { name: "Least left" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await order.getByRole("button", { name: "My order" }).click();
    const saved = (await host.callsTo("update_preferences")).at(-1)?.args as {
      readonly preferences: {
        readonly account_sort: string;
        readonly account_order: readonly string[];
      };
    };
    expect(saved.preferences.account_sort).toBe("manual");
    expect(saved.preferences.account_order).toHaveLength(overviewAccounts().length);
    await expect(order.getByRole("button", { name: "My order" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    // The buttons fade between states; the picture waits for them to settle.
    await page.waitForTimeout(300);
    await host.screenshotFull("settings-account-order");
  });

  test("the arrows are off unless the person arranges the accounts", async ({ open }) => {
    const host = await open(scenario("settings"), "#/settings/accounts");
    const down = host.page.getByRole("button", { name: /^Move .* down$/ }).first();
    await expect(down).toBeDisabled();
    await expect(down).toHaveAttribute(
      "title",
      "Choose My order in Appearance to arrange accounts",
    );
  });

  test("the arrows arrange accounts in My order", async ({ open }) => {
    const order = overviewAccounts().map((account) => account.account_id);
    const host = await open(
      scenario("settings", {
        preferences: defaultPreferences({ account_sort: "manual", account_order: order }),
      }),
      "#/settings/accounts",
    );
    const { page } = host;
    const cards = page.locator(".account-manage-card");
    const first = await cards.first().getAttribute("aria-label");
    await expect(
      page.getByRole("button", { name: /^Move .* up$/ }).first(),
    ).toBeDisabled();
    await page
      .getByRole("button", { name: /^Move .* down$/ })
      .first()
      .click();
    const saved = (await host.callsTo("update_preferences")).at(-1)?.args as {
      readonly preferences: { readonly account_order: readonly string[] };
    };
    expect(saved.preferences.account_order.slice(0, 2)).toEqual([order[1], order[0]]);
    // The moved account is now second.
    await expect(cards.nth(1)).toHaveAttribute("aria-label", first ?? "");
    await host.screenshotFull("settings-accounts-arranged");
  });

  test("the overview and the widget follow the arrangement", async ({ open }) => {
    const arranged = defaultPreferences({
      account_sort: "manual",
      account_order: reversedOrder(),
    });
    const host = await open(scenario("overview", { preferences: arranged }));
    const shown = await host.page
      .locator(".provider-card")
      .evaluateAll((cards) => cards.map((card) => card.getAttribute("data-account-id")));
    // An account whose monitoring is off still follows the rest.
    const off = overviewAccounts()
      .filter((account) => !account.monitoring_enabled)
      .map((account) => account.account_id);
    expect(shown).toEqual([...reversedOrder().filter((id) => !off.includes(id)), ...off]);
  });

  test("the widget follows the arrangement", async ({ open }) => {
    const order = reversedOrder();
    const host = await open(
      scenario("widget", {
        preferences: defaultPreferences({
          view: "widget",
          account_sort: "manual",
          account_order: order,
        }),
      }),
      "#/widget",
    );
    const names = await host.page
      .locator(".widget-tile")
      .evaluateAll((tiles) => tiles.map((tile) => tile.getAttribute("aria-label") ?? ""));
    // The first tile is the first arranged account that is monitored.
    const firstMonitored = overviewAccounts().find(
      (account) =>
        account.account_id ===
        order.find((id) =>
          overviewAccounts().some((a) => a.account_id === id && a.monitoring_enabled),
        ),
    );
    expect(names[0]?.toLowerCase()).toContain(firstMonitored?.provider_id ?? "?");
  });
});
