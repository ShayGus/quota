/**
 * Account groups in the real renderer: one OpenRouter account with several
 * keys, one card with the account's total and a ring for each key, a total
 * tile and a tile for each key in the widget, and putting keys together from
 * Settings.
 */
import { expect, test } from "./harness";
import { candidate, keyLimit } from "../fixtures";
import { defaultPreferences, openRouterGroup, scenario } from "./scenarios";

/** The Work group with its spend line hidden and its CI key left out. */
function hiddenCi() {
  return openRouterGroup().map((key) => {
    const group = key.group === null ? null : { ...key.group, spend_shown: false };
    return {
      ...key,
      group:
        key.account_id === "acct-or-ci" && group !== null
          ? { ...group, key_shown: false }
          : group,
    };
  });
}

/** A host that verifies one new OpenRouter key and holds it for the person. */
function newOpenRouterKey() {
  return {
    progress: [
      { kind: "started" as const },
      {
        kind: "awaiting_confirmation" as const,
        context: { candidate: candidate("openrouter", [keyLimit()]) },
      },
    ],
  };
}

test.describe("account groups", () => {
  test("the overview shows the account total once and a ring for each key", async ({
    open,
  }) => {
    const host = await open(scenario("overview", { accounts: openRouterGroup() }));
    const group = host.page.getByRole("article", { name: "OpenRouter Work account" });
    await expect(host.page.getByRole("article")).toHaveCount(1);
    await expect(group).toContainText("3 keys");
    await expect(group).toContainText("of $50.00 loaded");
    await expect(group).toContainText("Keys spent $4.30 today · $27.10 this month");
    const keys = group.getByRole("button", { name: /^Key OpenRouter / });
    await expect(keys).toHaveCount(3);
    await expect(keys.nth(0)).toContainText("Personal");
    await expect(keys.nth(0)).toContainText("58%");
    await expect(keys.nth(0)).toContainText("$11.60");
    // A key's ring shows its numbers alone, with no "left" after them.
    await expect(keys.nth(0)).not.toContainText("left");
    await expect(keys.nth(1)).toContainText("14%");
    // The balance is the account's, so it is drawn once, not for each key.
    await expect(group.getByText("of $50.00 loaded")).toHaveCount(1);
    await host.screenshot("group-overview");
    await host.screenshotFull("group-overview");
    // A key's ring opens that key's own details.
    await keys.nth(1).click();
    await expect(host.page.getByRole("article")).toHaveCount(0);
    await expect(host.page.getByText("CI", { exact: true })).toBeVisible();
    await expect(host.page.getByText("14% · 2.80 USD left")).toBeVisible();
    await host.screenshot("group-key-detail");
  });

  test("a key that must be reconnected says so on its own ring", async ({ open }) => {
    const accounts = openRouterGroup().map((key) =>
      key.account_id === "acct-or-ci"
        ? { ...key, connection_state: "reauthentication_required" as const }
        : key,
    );
    const host = await open(scenario("overview", { accounts }));
    const group = host.page.getByRole("article", { name: "OpenRouter Work account" });
    const ci = group.getByRole("button", { name: /^Reconnect OpenRouter CI/ });
    await expect(ci).toContainText("Reconnect");
    await expect(group).toContainText("CI: reconnect");
    await expect(group.getByRole("button", { name: /^Key OpenRouter / })).toHaveCount(2);
    await host.screenshot("group-overview-reconnect");
  });

  test("the compact layout lists the total, then a row for each key", async ({
    open,
  }) => {
    const host = await open(
      scenario("overview", {
        accounts: openRouterGroup(),
        preferences: defaultPreferences({ indicator_style: "bar" }),
      }),
    );
    const group = host.page.getByRole("article", { name: "OpenRouter Work account" });
    await expect(group.getByRole("button", { name: /^Key OpenRouter / })).toHaveCount(3);
    await host.screenshot("group-overview-bars");
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
    const group = host.page.getByRole("article", { name: "OpenRouter Group 1 account" });
    await expect(group).toBeVisible();
    await expect(group).not.toContainText("Work");
  });

  test("the widget shows the group's total, then a tile for each key", async ({
    open,
  }) => {
    const host = await open(
      scenario("widget", {
        accounts: openRouterGroup(),
        preferences: defaultPreferences({ view: "widget" }),
      }),
      "#/widget",
    );
    const tiles = host.page.locator(".widget-tile");
    await expect(tiles).toHaveCount(4);
    await expect(tiles.nth(0)).toHaveAccessibleName(/OpenRouter · Work/);
    await expect(tiles.nth(1)).toHaveAccessibleName(/Work · Personal/);
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

  test("a key and the spend line are hidden from Settings", async ({ open }) => {
    const host = await open(
      scenario("settings", { accounts: openRouterGroup() }),
      "#/settings/accounts",
    );
    const { page } = host;
    const ci = page.getByRole("switch", { name: "Show OpenRouter CI in its group" });
    await expect(ci).toHaveAttribute("aria-checked", "true");
    await ci.click();
    expect((await host.callsTo("set_group_key_shown")).at(-1)?.args).toEqual({
      accountRef: { id: "acct-or-ci" },
      shown: false,
    });
    await expect(ci).toHaveAttribute("aria-checked", "false");
    const spend = page.getByRole("switch", { name: "Show what the Work keys spent" });
    await spend.first().click();
    expect((await host.callsTo("set_group_spend_shown")).at(-1)?.args).toEqual({
      groupId: "group-work",
      shown: false,
    });
    // The spend line is the group's, so every key's switch follows it.
    for (const entry of await spend.all()) {
      await expect(entry).toHaveAttribute("aria-checked", "false");
    }
    // The switches slide; the picture waits for them to settle.
    await page.waitForTimeout(300);
    await host.screenshotFull("settings-group-display");
  });

  test("a hidden key and spend line leave the card", async ({ open }) => {
    const accounts = hiddenCi();
    const host = await open(scenario("overview", { accounts }));
    const group = host.page.getByRole("article", { name: "OpenRouter Work account" });
    await expect(group).toContainText("3 keys · 1 hidden");
    await expect(group).not.toContainText("Keys spent");
    await expect(group.getByRole("button", { name: /^Key OpenRouter / })).toHaveCount(2);
    // The balance is still the account's whole balance.
    await expect(group).toContainText("$37.20");
    await host.screenshot("group-overview-hidden");
  });

  // A test opens one host: a second one in the same test can keep the first's.
  test("a hidden key leaves the widget", async ({ open }) => {
    const host = await open(
      scenario("widget", {
        accounts: hiddenCi(),
        preferences: defaultPreferences({ view: "widget" }),
      }),
      "#/widget",
    );
    await expect(host.page.locator(".widget-tile")).toHaveCount(3);
  });

  test("a new key joins an existing group from the wizard", async ({ open }) => {
    const host = await open(
      scenario("settings", {
        accounts: openRouterGroup(),
        connection: newOpenRouterKey(),
      }),
      "#/settings/connect/1",
    );
    const { page } = host;
    await page.getByRole("button", { name: /OpenRouter/ }).click();
    await page.getByLabel("API key").fill("sk-or-v1-not-a-real-key");
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Add this account?" })).toBeVisible();
    const group = page.getByLabel("Account group");
    await expect(group).toHaveValue("");
    await group.selectOption({ label: "Work" });
    await page.getByLabel("Account nickname").fill("Batch");
    await host.screenshotFull("wizard-key-group");
    await page.getByRole("button", { name: "Add OpenRouter account" }).click();
    expect((await host.callsTo("confirm_connection")).at(-1)?.args).toEqual({
      attemptRef: expect.anything() as unknown,
      nickname: "Batch",
      group: { kind: "existing", group_id: "group-work" },
    });
    await expect(
      page.getByRole("combobox", { name: "Account group of OpenRouter Batch" }),
    ).toHaveValue("group-work");
  });

  test("a new key starts a new group from the wizard", async ({ open }) => {
    const host = await open(
      scenario("settings", { connection: newOpenRouterKey() }),
      "#/settings/connect/1",
    );
    const { page } = host;
    await page.getByRole("button", { name: /OpenRouter/ }).click();
    await page.getByLabel("API key").fill("sk-or-v1-not-a-real-key");
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await page.getByLabel("Account group").selectOption({ label: "New group…" });
    const add = page.getByRole("button", { name: "Add OpenRouter account" });
    // A new group needs its name first.
    await expect(add).toBeDisabled();
    await page.getByLabel("Group name").fill("Team");
    await add.click();
    expect((await host.callsTo("confirm_connection")).at(-1)?.args).toMatchObject({
      group: { kind: "new", name: "Team" },
    });
  });

  test("Add key on a group's card opens the wizard on that group", async ({ open }) => {
    const host = await open(scenario("overview", { accounts: openRouterGroup() }));
    await host.page.getByRole("button", { name: "Add a key to OpenRouter Work" }).click();
    expect((await host.callsTo("open_settings_window")).at(-1)?.args).toEqual({
      destination: { add_key: { group_id: "group-work" } },
    });
  });

  test("the add-key route starts on the group's provider with it chosen", async ({
    open,
  }) => {
    const host = await open(
      scenario("settings", {
        accounts: openRouterGroup(),
        connection: newOpenRouterKey(),
      }),
      "#/settings/connect/1/group-work",
    );
    const { page } = host;
    await expect(page.getByRole("heading", { name: "Add a key to Work" })).toBeVisible();
    await host.screenshot("wizard-add-key");
    await page.getByLabel("API key").fill("sk-or-v1-not-a-real-key");
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect(page.getByLabel("Account group")).toHaveValue("group-work");
  });

  test("Add key in Settings opens the wizard on that group", async ({ open }) => {
    const host = await open(
      scenario("settings", { accounts: openRouterGroup() }),
      "#/settings/accounts",
    );
    const { page } = host;
    await page
      .getByRole("article", { name: "Manage OpenRouter Personal" })
      .getByRole("button", { name: "Add key" })
      .click();
    await expect(page.getByRole("heading", { name: "Add a key to Work" })).toBeVisible();
  });

  test("a key that may not read the balance still counts in what the keys spent", async ({
    open,
  }) => {
    // CI's key may not read the credits: no balance and no balance window.
    const accounts = openRouterGroup().map((key) =>
      key.account_id === "acct-or-ci"
        ? {
            ...key,
            balance: null,
            windows: key.windows.filter(
              (window) => window.metric_role !== "prepaid_balance",
            ),
          }
        : key,
    );
    const host = await open(scenario("overview", { accounts }));
    const group = host.page.getByRole("article", { name: "OpenRouter Work account" });
    await expect(group).toContainText("Keys spent $4.30 today · $27.10 this month");
    const ci = group.getByRole("button", { name: /^Key OpenRouter CI/ });
    await expect(ci).toContainText("$2.80");
    // Its details say what it spent, though it has no balance to show.
    await ci.click();
    await expect(host.page.getByLabel("This key")).toContainText("$17.20 this month");
    await host.screenshotFull("key-detail-no-balance");
  });
});
