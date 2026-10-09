/**
 * Account groups in the real renderer: one OpenRouter account with several
 * keys, its total over its keys in the overview, one tile in the widget, and
 * putting keys together from Settings.
 */
import { expect, test } from "./harness";
import { defaultPreferences, openRouterGroup, scenario } from "./scenarios";

test.describe("account groups", () => {
  test("the overview shows the account total once and each key's own limit", async ({
    open,
  }) => {
    const host = await open(scenario("overview", { accounts: openRouterGroup() }));
    const group = host.page.getByRole("region", { name: "OpenRouter Work account" });
    await expect(group).toContainText("Work");
    await expect(group).toContainText("3 keys");
    await expect(group).toContainText("$37.20 left");
    await expect(group).toContainText("Keys spent $4.30 today · $27.10 this month");
    const keys = group.getByRole("article");
    await expect(keys).toHaveCount(3);
    // Each key shows its own limit; the account balance is not repeated.
    for (const key of await keys.all()) {
      await expect(key).toContainText("API key limit");
      await expect(key).not.toContainText("of $50.00 loaded");
    }
    await host.screenshot("group-overview");
    await host.screenshotFull("group-overview");
  });

  test("hidden account names hide the group's name too", async ({ open }) => {
    const host = await open(
      scenario("overview", {
        accounts: openRouterGroup(),
        preferences: defaultPreferences({
          privacy: { ...defaultPreferences().privacy, alias_mode: "stable_aliases" },
        }),
      }),
    );
    const group = host.page.getByRole("region", { name: "OpenRouter Group 1 account" });
    await expect(group).toBeVisible();
    await expect(group).not.toContainText("Work");
  });

  test("the widget shows the group as one tile named for it", async ({ open }) => {
    const host = await open(
      scenario("widget", {
        accounts: openRouterGroup(),
        preferences: defaultPreferences({ view: "widget" }),
      }),
      "#/widget",
    );
    const tiles = host.page.locator(".widget-tile");
    await expect(tiles).toHaveCount(1);
    await expect(tiles.first()).toHaveAccessibleName(/OpenRouter · Work/);
    await host.screenshot("group-widget");
  });

  test("a key is put in a new group and another joins it from Settings", async ({
    open,
  }) => {
    const ungrouped = openRouterGroup().map((key) => ({ ...key, group: null }));
    const host = await open(
      scenario("settings", { accounts: ungrouped }),
      "#/settings/accounts",
    );
    const { page } = host;
    await page
      .getByRole("combobox", { name: "Account group of OpenRouter Personal" })
      .selectOption({ label: "New group…" });
    await page.getByLabel("Group name").fill("Work");
    await page.getByRole("button", { name: "Create group" }).click();
    expect((await host.callsTo("create_account_group")).at(-1)?.args).toEqual({
      name: "Work",
      accountRefs: [{ id: "acct-or-personal" }],
    });
    const ci = page.getByRole("combobox", { name: "Account group of OpenRouter CI" });
    await expect(ci.getByRole("option", { name: "Work" })).toHaveCount(1);
    await ci.selectOption({ label: "Work" });
    const joined = (await host.callsTo("set_account_group")).at(-1)?.args as
      | { readonly accountRef: { readonly id: string }; readonly groupId: string }
      | undefined;
    expect(joined?.accountRef).toEqual({ id: "acct-or-ci" });
    await expect(ci).toHaveValue(joined?.groupId ?? "");
    await host.screenshotFull("settings-account-group");
  });
});
