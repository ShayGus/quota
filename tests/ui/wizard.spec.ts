/**
 * The add-account wizard, step by step against a scripted host: Provider,
 * Connect, Verify. The faked host stands in for the provider; what is checked
 * is what the renderer asks for and what it shows at each step.
 */
import { expect, test } from "./harness";
import { candidate, percent, window as quotaWindow } from "../fixtures";
import { defaultPreferences, scenario, verifiedConnection } from "./scenarios";

const CONNECT = "#/settings/connect/1";

test.describe("add-account wizard", () => {
  test("Provider step lists every provider and Cancel leaves", async ({ open }) => {
    const host = await open(scenario("settings"), CONNECT);
    const { page } = host;
    await expect(page.getByRole("heading", { name: "Add a subscription" })).toBeVisible();
    await expect(page.locator(".step.selected")).toHaveText("1");
    await expect(page.getByRole("button", { name: /Codex/ })).toBeVisible();
    await expect(page.locator(".provider-pick")).toHaveCount(12);
    await host.screenshot("wizard-1-provider");
    await host.screenshotFull("wizard-provider");
    await page.getByRole("button", { name: "Cancel" }).click();
    await expect(page.getByRole("heading", { name: "Accounts", level: 3 })).toBeVisible();
  });

  test("local sign-in: Provider, Connect, Verify, then Add saves the account", async ({
    open,
  }) => {
    const host = await open(
      scenario("settings", { connection: verifiedConnection() }),
      CONNECT,
    );
    const { page } = host;
    await page.getByRole("button", { name: /Codex/ }).click();
    await expect(page.getByRole("heading", { name: "Connect Codex" })).toBeVisible();
    await expect(page.locator(".step.selected")).toHaveText("2");
    await host.screenshot("wizard-2-connect");
    await host.screenshotFull("wizard-connect");

    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Add this account?" })).toBeVisible();
    await expect(page.locator(".step.selected")).toHaveText("3");
    await expect(page.getByText("new.person@example.test")).toBeVisible();
    await expect(page.getByText("5-hour")).toBeVisible();
    await host.screenshot("wizard-3-verify");
    await host.screenshotFull("wizard-verify");
    // Nothing is saved until the person adds it.
    expect(await host.callsTo("confirm_connection")).toHaveLength(0);

    await page.getByRole("textbox").fill("Night shift");
    await page.getByRole("button", { name: "Add Codex account" }).click();
    await expect(page.getByRole("heading", { name: "Accounts", level: 3 })).toBeVisible();
    await expect(page.getByLabel("Manage Codex Night shift")).toBeVisible();
    expect((await host.callsTo("confirm_connection")).at(-1)?.args).toEqual({
      attemptRef: { id: "attempt-1" },
      nickname: "Night shift",
    });
    const begin = (await host.callsTo("begin_connection")).at(-1)?.args as {
      request: { provider_id: string; credential: unknown };
    };
    expect(begin.request.provider_id).toBe("codex");
    expect(begin.request.credential).toBeNull();
  });

  test("Not this account discards the candidate and says how to switch", async ({
    open,
  }) => {
    const host = await open(
      scenario("settings", { connection: verifiedConnection() }),
      CONNECT,
    );
    const { page } = host;
    await page.getByRole("button", { name: /Codex/ }).click();
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await page.getByRole("button", { name: "Not this account" }).click();
    await expect(page.getByRole("heading", { name: "Connect Codex" })).toBeVisible();
    await expect(page.getByText(/To add a different Codex account/)).toBeVisible();
    expect((await host.callsTo("cancel_connection")).length).toBeGreaterThan(0);
    expect(await host.callsTo("confirm_connection")).toHaveLength(0);
  });

  test("an API-key provider needs a key and hands it to the host", async ({ open }) => {
    const host = await open(
      scenario("settings", { connection: verifiedConnection() }),
      CONNECT,
    );
    const { page } = host;
    await page.getByRole("button", { name: /OpenRouter/ }).click();
    const connect = page.getByRole("button", { name: "Connect", exact: true });
    await expect(connect).toBeDisabled();
    await page.getByLabel("API key").fill("sk-or-v1-not-a-real-key");
    await expect(connect).toBeEnabled();
    await connect.click();
    await expect(page.getByRole("heading", { name: "Add this account?" })).toBeVisible();
    const begin = (await host.callsTo("begin_connection")).at(-1)?.args as {
      request: { credential: string };
    };
    expect(begin.request.credential).toBe("sk-or-v1-not-a-real-key");
  });

  for (const theme of ["light", "dark"] as const) {
    test(`OpenCode Go takes a pasted key and the account is added (${theme})`, async ({
      open,
    }) => {
      const host = await open(
        scenario("settings", {
          preferences: defaultPreferences({ theme }),
          connection: {
            progress: [
              { kind: "started" },
              {
                kind: "awaiting_confirmation",
                context: {
                  candidate: candidate("open_code_go", [
                    quotaWindow("oc-5h", "session", percent(64), { label: "5-hour" }),
                  ]),
                },
              },
            ],
          },
        }),
        CONNECT,
      );
      const { page } = host;
      await page.getByRole("button", { name: /OpenCode Go/ }).click();
      await expect(
        page.getByRole("heading", { name: "Connect OpenCode Go" }),
      ).toBeVisible();
      await expect(page.locator(".badge")).toHaveText("API KEY");
      const key = page.getByLabel("API key");
      await expect(key).toHaveAttribute("type", "password");
      await expect(key).toHaveAttribute("placeholder", "Your OpenCode API key");
      await expect(
        page.getByText(/leave it empty to use the sign-in the OpenCode CLI/),
      ).toBeVisible();
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      await host.screenshot(`wizard-opencode-key-${theme}`);
      await host.screenshotFull(`wizard-api-key-${theme}`);
      await key.fill("  oc-go-not-a-real-key  ");
      await page.getByRole("button", { name: "Connect", exact: true }).click();
      await expect(
        page.getByRole("heading", { name: "Add this account?" }),
      ).toBeVisible();
      const begins = await host.callsTo("begin_connection");
      expect(begins).toHaveLength(1);
      const begin = begins[0]?.args as {
        request: { credential: string };
      };
      expect(begin.request.credential).toBe("oc-go-not-a-real-key");
      await page.getByRole("textbox").fill("Go");
      await page.getByRole("button", { name: "Add OpenCode Go account" }).click();
      await expect(
        page.getByRole("heading", { name: "Accounts", level: 3 }),
      ).toBeVisible();
      await expect(page.getByLabel("Manage OpenCode Go Go")).toBeVisible();
      await host.screenshot(`wizard-opencode-saved-${theme}`);
      const confirmations = await host.callsTo("confirm_connection");
      expect(confirmations).toHaveLength(1);
      const confirm = confirmations[0]?.args as {
        nickname: string;
      };
      expect(confirm.nickname).toBe("Go");
    });
  }

  test("OpenCode Go with no pasted key signs in through its CLI", async ({ open }) => {
    const host = await open(
      scenario("settings", { connection: { progress: [{ kind: "started" }] } }),
      CONNECT,
    );
    const { page } = host;
    await page.getByRole("button", { name: /OpenCode Go/ }).click();
    await expect(
      page.getByText(/leave it empty to use the sign-in the OpenCode CLI/),
    ).toBeVisible();
    const connect = page.getByRole("button", { name: "Connect", exact: true });
    await expect(connect).toBeEnabled();
    await connect.click();
    await expect(page.getByRole("button", { name: "Verifying…" })).toBeVisible();
    const begin = (await host.callsTo("begin_connection")).at(-1)?.args as {
      request: { credential: unknown; provider_id: string };
    };
    expect(begin.request.provider_id).toBe("open_code_go");
    expect(begin.request.credential).toBeNull();
  });

  test("an OpenCode Go key the provider refuses explains how to recover", async ({
    open,
  }) => {
    const base = defaultPreferences();
    const host = await open(
      scenario("settings", {
        preferences: { ...base, theme: "light" },
        connection: {
          progress: [
            { kind: "started" },
            { kind: "failed", context: { error: { kind: "reconnect_required" } } },
          ],
        },
      }),
      CONNECT,
    );
    const { page } = host;
    await page.getByRole("button", { name: /OpenCode Go/ }).click();
    await page.getByLabel("API key").fill("oc-go-refused");
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect(page.getByRole("alert")).toContainText("opencode.ai/auth");
    await expect(page.getByRole("alert")).toContainText("opencode auth login");
    await expect(page.getByRole("alert")).toContainText("press Connect again");
    await host.screenshot("wizard-opencode-refused-light");
  });

  for (const provider of ["Codex", "Claude", "Cursor"] as const) {
    test(`local-only providers still offer no key box (${provider})`, async ({
      open,
    }) => {
      const host = await open(scenario("settings"), CONNECT);
      const { page } = host;
      await page.getByRole("button", { name: new RegExp(`^${provider}`) }).click();
      await expect(page.getByLabel("API key")).toHaveCount(0);
      await expect(page.locator(".badge")).toHaveText("LOCAL SIGN-IN");
      await expect(
        page.getByText(
          "Quota uses this provider's existing local sign-in to read your quota.",
        ),
      ).toBeVisible();
    });
  }

  test("a browser sign-in shows the code to enter", async ({ open }) => {
    const host = await open(
      scenario("settings", {
        connection: {
          progress: [
            { kind: "started" },
            {
              kind: "awaiting_user",
              context: {
                sign_in: {
                  user_code: "WXYZ-1234",
                  verification_uri: "https://accounts.example.test/device",
                },
              },
            },
          ],
        },
      }),
      CONNECT,
    );
    const { page } = host;
    await page.getByRole("button", { name: /Grok/ }).click();
    await page.getByRole("button", { name: "Sign in with browser" }).click();
    await expect(page.getByText("WXYZ-1234")).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Waiting for the browser…" }),
    ).toBeDisabled();
    await host.screenshot("wizard-browser-code");
    await host.screenshotFull("wizard-browser-code");
    const begin = (await host.callsTo("begin_connection")).at(-1)?.args as {
      request: { browser_sign_in: boolean };
    };
    expect(begin.request.browser_sign_in).toBe(true);
  });

  test("a website sign-in opens the provider's site in a window and waits", async ({
    open,
  }) => {
    const host = await open(
      scenario("settings", {
        connection: {
          progress: [
            { kind: "started" },
            { kind: "awaiting_user", context: { sign_in: null } },
          ],
        },
      }),
      CONNECT,
    );
    const { page } = host;
    await page.getByRole("button", { name: /TypeSafe/ }).click();
    await expect(page.locator(".badge")).toHaveText("WEBSITE SIGN-IN");
    await expect(
      page.getByRole("heading", { name: "Sign in on console.typesafe.ai" }),
    ).toBeVisible();
    // The sign-in opens the person's own browser, so Google sign-in works there.
    await expect(
      page.getByText(/separate Chrome or Edge window, just for Quota/),
    ).toBeVisible();
    await expect(page.getByText(/Without Chrome or Edge/)).toBeVisible();
    await host.screenshotFull("wizard-website-sign-in");
    await page.getByRole("button", { name: "Sign in to TypeSafe" }).click();
    await expect(
      page.getByText("Sign in to console.typesafe.ai in the window Quota opened"),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Waiting for the sign-in…" }),
    ).toBeDisabled();
    await expect(
      page.getByText("Finish signing in with the provider's own tool."),
    ).toHaveCount(0);
    await host.screenshotFull("wizard-website-waiting");
    const begin = (await host.callsTo("begin_connection")).at(-1)?.args as {
      request: { browser_sign_in: boolean; credential: string | null };
    };
    expect(begin.request.browser_sign_in).toBe(true);
    expect(begin.request.credential).toBeNull();
  });

  test("a sign-in the provider refuses explains how to recover", async ({ open }) => {
    const host = await open(
      scenario("settings", {
        connection: {
          progress: [
            { kind: "started" },
            { kind: "failed", context: { error: { kind: "reconnect_required" } } },
          ],
        },
      }),
      CONNECT,
    );
    const { page } = host;
    await page.getByRole("button", { name: /Codex/ }).click();
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect(page.getByRole("alert")).toContainText("run codex login");
    await host.screenshot("wizard-failed-sign-in");
  });

  test("a refused attempt is shown on the Connect step", async ({ open }) => {
    const host = await open(
      scenario("settings", {
        connection: {
          progress: [],
          refuseWith: {
            kind: "validation_failed",
            context: { field: "profile", reason: "duplicate" },
          },
        },
      }),
      CONNECT,
    );
    const { page } = host;
    await page.getByRole("button", { name: /Codex/ }).click();
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect(page.getByText("The connection was refused.")).toBeVisible();
  });

  test("Back returns to the provider list", async ({ open }) => {
    const host = await open(scenario("settings"), CONNECT);
    const { page } = host;
    await page.getByRole("button", { name: /Claude/ }).click();
    await page.getByRole("button", { name: "Back" }).click();
    await expect(page.getByRole("heading", { name: "Add a subscription" })).toBeVisible();
  });

  test("with aliases on, the verified identity is hidden", async ({ open }) => {
    const base = defaultPreferences();
    const host = await open(
      scenario("settings", {
        connection: verifiedConnection(),
        preferences: {
          ...base,
          privacy: { ...base.privacy, alias_mode: "stable_aliases" },
        },
      }),
      CONNECT,
    );
    const { page } = host;
    await page.getByRole("button", { name: /Codex/ }).click();
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Add this account?" })).toBeVisible();
    await expect(page.getByText("new.person@example.test")).toHaveCount(0);
    await expect(page.getByText("Workspace hidden")).toBeVisible();
  });
});
