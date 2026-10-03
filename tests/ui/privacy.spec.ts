/**
 * The privacy alias display: with "Hide account names" on, no real nickname or
 * address appears anywhere the renderer shows an account.
 */
import { expect, test } from "./harness";
import { defaultPreferences, scenario } from "./scenarios";

const REAL = [
  "Work Claude",
  "Personal Codex",
  "Studio Cursor",
  "acct-claude@example.test",
];

test.describe("privacy aliases", () => {
  test("the switch saves the alias mode through the host", async ({ open }) => {
    const host = await open(scenario("settings"), "#/settings/privacy");
    const hide = host.page.getByRole("switch", { name: "Hide account names" });
    await expect(hide).toHaveAttribute("aria-checked", "false");
    await hide.click();
    await expect(hide).toHaveAttribute("aria-checked", "true");
    const saved = (await host.callsTo("update_preferences")).at(-1)?.args as {
      preferences: { privacy: { alias_mode: string } };
    };
    expect(saved.preferences.privacy.alias_mode).toBe("stable_aliases");
  });

  test("the popover shows stable, distinct aliases and no real names", async ({
    open,
  }) => {
    const base = defaultPreferences();
    const host = await open(
      scenario("overview", {
        preferences: {
          ...base,
          privacy: { ...base.privacy, alias_mode: "stable_aliases" },
        },
      }),
    );
    const { page } = host;
    await expect(page.getByRole("article")).toHaveCount(7);
    const text = await page.locator("body").innerText();
    for (const real of REAL) expect(text).not.toContain(real);
    const aliases = new Set(text.match(/Account \d/g));
    expect(aliases.size).toBe(7);
    await host.screenshot("popover-aliases");
  });

  test("turning aliases on after load replaces names without a reload", async ({
    open,
  }) => {
    const host = await open(scenario("overview"));
    const { page } = host;
    await expect(page.getByLabel(/Work Claude allowance/)).toBeVisible();
    const { preferences } = await host.hostState();
    await host.emit("preferences-changed", {
      app_instance_id: "ui-test-instance",
      preference_revision: preferences.revision + 1,
      preferences: {
        ...preferences,
        revision: preferences.revision + 1,
        privacy: { ...preferences.privacy, alias_mode: "stable_aliases" },
      },
    });
    await expect(page.getByLabel(/Work Claude allowance/)).toHaveCount(0);
    await expect(page.getByLabel(/Claude Account \d allowance/)).toBeVisible();
  });

  test("the settings accounts list hides identities and disables rename", async ({
    open,
  }) => {
    const base = defaultPreferences();
    const host = await open(
      scenario("settings", {
        preferences: {
          ...base,
          privacy: { ...base.privacy, alias_mode: "stable_aliases" },
        },
      }),
      "#/settings/accounts",
    );
    const text = await host.page.locator("body").innerText();
    for (const real of REAL) expect(text).not.toContain(real);
    await expect(host.page.getByText("Identity hidden").first()).toBeVisible();
    await expect(
      host.page.getByRole("button", { name: "Rename" }).first(),
    ).toBeDisabled();
    await host.screenshot("settings-accounts-aliases");
  });
});
