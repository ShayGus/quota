/**
 * The settings window: every panel opens, and each control states a preference
 * by asking the host, then shows only what the host confirms.
 */
import { expect, test } from "./harness";
import { scenario } from "./scenarios";

const PANELS = [
  ["general", "General"],
  ["accounts", "Accounts"],
  ["appearance", "Appearance"],
  ["notifications", "Notifications"],
  ["privacy", "Privacy"],
  ["diagnostics", "Diagnostics"],
] as const;

test.describe("settings", () => {
  test("every panel opens from the rail", async ({ open }) => {
    const host = await open(scenario("settings"), "#/settings/general");
    const { page } = host;
    await expect(page.getByRole("heading", { name: "Quota settings" })).toBeVisible();
    for (const [id, title] of PANELS) {
      await page
        .getByRole("navigation", { name: "Settings sections" })
        .getByRole("button", { name: title, exact: true })
        .click();
      await expect(page.getByRole("heading", { name: title, level: 3 })).toBeVisible();
      await host.screenshot(`settings-${id}`);
      await host.screenshotFull(`settings-${id}`);
    }
  });

  test("settings wait for the host's preferences instead of guessing", async ({
    open,
  }) => {
    const host = await open(
      scenario("settings", {
        refuse: {
          get_snapshot: {
            kind: "persistence_unavailable",
            context: { owner: "history" },
          },
        },
      }),
      "#/settings/general",
    );
    await expect(
      host.page.getByText("Quota has not received the confirmed preferences"),
    ).toBeVisible();
  });

  test("pausing monitoring asks the host and shows the confirmed state", async ({
    open,
  }) => {
    const host = await open(scenario("settings"), "#/settings/general");
    const pause = host.page.getByRole("switch", { name: "Pause monitoring" });
    await expect(pause).toHaveAttribute("aria-checked", "false");
    await pause.click();
    await expect(pause).toHaveAttribute("aria-checked", "true");
    expect((await host.callsTo("set_monitoring_state")).at(-1)?.args).toEqual({
      paused: true,
    });
  });

  test("launch at login goes through the autostart plugin", async ({ open }) => {
    const host = await open(scenario("settings"), "#/settings/general");
    const toggle = host.page.getByRole("switch", { name: "Launch at login" });
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    expect((await host.callsTo("plugin:autostart|enable")).length).toBe(1);
  });

  test("always on top is its own command", async ({ open }) => {
    const host = await open(scenario("settings"), "#/settings/general");
    await host.page.getByRole("switch", { name: "Always on top" }).click();
    expect((await host.callsTo("set_overview_always_on_top")).at(-1)?.args).toEqual({
      alwaysOnTop: true,
    });
  });

  test("a refused preference save is stated, not silent", async ({ open }) => {
    const host = await open(
      scenario("settings", {
        refuse: {
          update_preferences: {
            kind: "revision_conflict",
            context: { expected: 7, actual: 8 },
          },
        },
      }),
      "#/settings/appearance",
    );
    await host.page.getByRole("button", { name: "Light", exact: true }).click();
    await expect(host.page.getByText("The settings changed elsewhere")).toBeVisible();
  });

  test.describe("accounts", () => {
    test("lists every account and toggles monitoring", async ({ open }) => {
      const host = await open(scenario("settings"), "#/settings/accounts");
      const { page } = host;
      await expect(page.getByRole("article")).toHaveCount(7);
      const codex = page.getByRole("switch", { name: "Monitor Codex Personal Codex" });
      await expect(codex).toHaveAttribute("aria-checked", "true");
      await codex.click();
      await expect(codex).toHaveAttribute("aria-checked", "false");
    });

    test("rename saves the new nickname", async ({ open }) => {
      const host = await open(scenario("settings"), "#/settings/accounts");
      const { page } = host;
      await page
        .getByLabel("Manage Codex Personal Codex")
        .getByRole("button", { name: "Rename" })
        .click();
      const dialog = page.getByRole("dialog", { name: "Rename account" });
      await dialog.getByRole("textbox").fill("Renamed Codex");
      await dialog.getByRole("button", { name: "Save nickname" }).click();
      await expect(page.getByLabel("Manage Codex Renamed Codex")).toBeVisible();
      expect((await host.callsTo("rename_account")).at(-1)?.args).toEqual({
        accountRef: { id: "acct-codex" },
        nickname: "Renamed Codex",
      });
    });

    test("disconnect asks first and removes only that account", async ({ open }) => {
      const host = await open(scenario("settings"), "#/settings/accounts");
      const { page } = host;
      await page
        .getByLabel("Manage Cursor Studio Cursor")
        .getByRole("button", { name: "Disconnect" })
        .click();
      await host.screenshot("settings-disconnect-dialog");
      await page.getByRole("dialog").getByRole("button", { name: "Disconnect" }).click();
      await expect(page.getByRole("article")).toHaveCount(6);
      await expect(page.getByLabel("Manage Cursor Studio Cursor")).toHaveCount(0);
    });
  });

  test("diagnostics export reports where it saved", async ({ open }) => {
    const host = await open(scenario("settings"), "#/settings/diagnostics");
    await host.page.getByRole("button", { name: "Export diagnostics" }).click();
    await expect(
      host.page.getByText("Saved to /tmp/quota-diagnostics.json"),
    ).toBeVisible();
  });

  test("notifications switch saves the whole preference object", async ({ open }) => {
    const host = await open(scenario("settings"), "#/settings/notifications");
    await host.page.getByRole("switch", { name: "Enable notifications" }).click();
    const saved = (await host.callsTo("update_preferences")).at(-1)?.args as {
      preferences: { notifications: { enabled: boolean } };
    };
    expect(saved.preferences.notifications.enabled).toBe(true);
  });
});
