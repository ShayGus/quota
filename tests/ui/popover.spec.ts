/**
 * The main popover: several accounts, each in the state the UI really has.
 */
import { expect, test } from "./harness";
import { overviewAccounts, scenario, unavailableAccount } from "./scenarios";

test.describe("popover", () => {
  test("lists every account with its own state", async ({ open }) => {
    const host = await open(scenario("overview"));
    const { page } = host;
    await expect(page.getByRole("heading", { name: "Quota", exact: true })).toBeVisible();
    const cards = page.getByRole("article");
    await expect(cards).toHaveCount(7);

    const card = (name: string) => page.getByLabel(new RegExp(`${name} allowance`));
    await expect(card("Work Claude")).toContainText("Current");
    await expect(card("Personal Codex")).toContainText("5h low");
    await expect(card("Studio Cursor")).toContainText("Rate limited");
    await expect(card("Side project")).toContainText("Offline");
    await expect(card("Team Z.ai")).toContainText("Check failed");
    await expect(card("Old Kimi")).toContainText("Reconnect");
    await expect(card("Parked Grok")).toContainText("Monitoring off");
    await host.screenshot("popover-seven-accounts");
    await host.screenshotFull("overview-all");
  });

  test("the attention filter keeps only accounts that need something", async ({
    open,
  }) => {
    const host = await open(scenario("overview"));
    const { page } = host;
    await page.getByRole("button", { name: /^Attention/ }).click();
    await expect(page.getByRole("article")).toHaveCount(5);
    await expect(page.getByLabel(/Work Claude allowance/)).toHaveCount(0);
    await expect(page.getByLabel(/Parked Grok allowance/)).toHaveCount(0);
    await host.screenshot("popover-attention-filter");
    await host.screenshotFull("attention-filter");
  });

  test("a card opens its detail and Back returns to the list", async ({ open }) => {
    const host = await open(scenario("overview"));
    const { page } = host;
    await page.getByRole("button", { name: /Details for Codex Personal Codex/ }).click();
    await expect(page.getByRole("article")).toHaveCount(0);
    await host.screenshot("popover-account-detail");
    await host.screenshotFull("account-detail");
    await page.keyboard.press("Escape");
    await expect(page.getByRole("article")).toHaveCount(7);
  });

  test("refresh asks the host and says so", async ({ open }) => {
    const host = await open(scenario("overview"));
    await host.page.getByRole("button", { name: "Refresh readings" }).click();
    await expect
      .poll(async () => (await host.callsTo("refresh_accounts")).length)
      .toBe(1);
  });

  test("paused monitoring is stated and Resume asks the host", async ({ open }) => {
    const host = await open(
      scenario("overview", { snapshot: { monitoring_state: { kind: "paused" } } }),
    );
    const { page } = host;
    await expect(
      page.getByText("Monitoring is paused. Values are last known."),
    ).toBeVisible();
    await page.getByRole("button", { name: "Resume" }).click();
    await expect(page.getByText("Monitoring is paused.")).toHaveCount(0);
    expect((await host.callsTo("set_monitoring_state")).at(-1)?.args).toEqual({
      paused: false,
    });
  });

  test("a host that cannot be reached is stated, not blank", async ({ open }) => {
    const host = await open(
      scenario("overview", {
        refuse: {
          get_snapshot: {
            kind: "persistence_unavailable",
            context: { owner: "history" },
          },
        },
      }),
    );
    await expect(host.page.getByText("Quota is not reachable")).toBeVisible();
    await host.screenshot("popover-host-unreachable");
  });

  test("first launch with no account explains itself and offers Add", async ({
    open,
  }) => {
    const host = await open(scenario("overview", { accounts: [] }));
    const { page } = host;
    await expect(page.getByText("Your subscriptions,")).toBeVisible();
    await page.getByRole("button", { name: "Add your first account" }).click();
    await expect
      .poll(async () => (await host.callsTo("open_settings_window")).length)
      .toBe(1);
    expect((await host.callsTo("open_settings_window"))[0]?.args).toEqual({
      destination: "connect",
    });
    await host.screenshot("popover-first-launch");
    await host.screenshotFull("first-launch");
  });

  test("an account with an unreported reading still renders", async ({ open }) => {
    const host = await open(
      scenario("overview", { accounts: [...overviewAccounts(), unavailableAccount()] }),
    );
    await expect(host.page.getByRole("article")).toHaveCount(8);
  });
});
