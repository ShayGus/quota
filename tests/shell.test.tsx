/**
 * Theme, pin, always-on-top, navigation, and boundary behaviour.
 *
 * These exercise the whole window, with the IPC boundary replaced by a test
 * double, so the renderer's own behaviour is what is measured (spec 17.1).
 */
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

/** The recorded calls to the generated bindings' underlying transport. */
const invoked: { command: string; args: unknown }[] = [];

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: unknown) => {
    invoked.push({ command, args });
    // A started connection answers with its attempt identity, as the host does.
    if (command === "begin_connection") {
      return Promise.resolve({
        attempt_ref: { id: "attempt-1" },
        attempt_id: "attempt-1",
      });
    }
    return Promise.resolve(null);
  },
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    close: () => {
      invoked.push({ command: "native-close", args: null });
      return Promise.resolve();
    },
    show: () => Promise.resolve(),
    setFocus: () => Promise.resolve(),
  }),
}));

/** The login item as the operating system double reports it. */
const loginItem = vi.hoisted(() => ({ enabled: false, refuse: false }));

vi.mock("@tauri-apps/plugin-autostart", () => ({
  isEnabled: () => Promise.resolve(loginItem.enabled),
  enable: () => {
    if (loginItem.refuse) return Promise.reject(new Error("refused"));
    loginItem.enabled = true;
    return Promise.resolve();
  },
  disable: () => {
    loginItem.enabled = false;
    return Promise.resolve();
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
  emitTo: (target: string, event: string, payload: unknown) => {
    invoked.push({ command: "emit-to", args: { target, event, payload } });
    return Promise.resolve();
  },
}));

// `vi.mock` is hoisted above these imports by the test runner, so the transport
// is replaced before the module graph under test is evaluated.
import { App } from "../src/app/App";
import { FeatureBoundary } from "../src/app/ErrorBoundary";
import {
  acceptMonitoring,
  acceptPreferences,
  acceptSnapshot,
  setFailure,
} from "../src/shared/state/store";
import type { PollingStrategy, ProviderPollingPolicy } from "../src/generated/bindings";
import {
  account,
  percent,
  preferences,
  snapshot,
  window as quotaWindow,
} from "./fixtures";

/** One account with a single session window. */
function oneAccount(): ReturnType<typeof account>[] {
  return [
    account("a1", "codex", 1, [quotaWindow("w", "session", percent(72))], {
      rank: 72,
    }),
  ];
}

/** The recorded calls to one command. */
function commandsMatching(command: string): typeof invoked {
  return invoked.filter((call) => call.command === command);
}

/** Every recorded call except the one-time snapshot reconciliation at mount. */
const mutatingCommands = (): readonly string[] =>
  invoked.map((call) => call.command).filter((command) => command !== "get_snapshot");

beforeEach(() => {
  invoked.length = 0;
  window.location.hash = "";
});

describe("the colour scheme", () => {
  it("applies the light scheme when the preference says light", async () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ theme: "light" }));
    render(<App />);

    await waitFor(() => {
      expect(document.documentElement.dataset["theme"]).toBe("light");
    });
  });

  it("applies the dark scheme when the preference says dark", async () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ theme: "dark" }));
    render(<App />);

    await waitFor(() => {
      expect(document.documentElement.dataset["theme"]).toBe("dark");
    });
  });
});

describe("the pin and always-on-top controls", () => {
  it("floats the popover as a separate window and sends nothing else", async () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ overview_mode: "tray", always_on_top: false }));
    render(<App />);

    const pin = screen.getByRole("button", { name: "Float as a separate window" });
    expect(pin.getAttribute("aria-pressed")).toBe("false");
    await act(async () => {
      pin.click();
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(commandsMatching("set_overview_mode")).toHaveLength(1);
    });
    expect(commandsMatching("set_overview_mode")[0]?.args).toEqual({ mode: "floating" });
    // Floating never changes topmost: that is a separate setting.
    expect(mutatingCommands()).toEqual(["set_overview_mode"]);
  });

  it("states the floating state in words as well as in colour", () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ overview_mode: "floating" }));
    render(<App />);

    expect(screen.getByText("Floating")).toBeTruthy();
    const pin = screen.getByRole("button", { name: "Dock to the tray" });
    expect(pin.getAttribute("aria-pressed")).toBe("true");
  });

  it("keeps the window on top from settings in either mode, sending only that preference", async () => {
    window.location.hash = "#/settings/general";
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ always_on_top: false }));
    render(<App />);

    const topmost = screen.getByRole("switch", { name: "Always on top" });
    await act(() => fireEvent.click(topmost));
    // Tauri names command arguments after the Rust parameter.
    expect(commandsMatching("set_overview_always_on_top")[0]?.args).toEqual({
      alwaysOnTop: true,
    });
    expect(mutatingCommands()).toEqual(["set_overview_always_on_top"]);
  });
});

describe("a render failure", () => {
  it("contains the failure without clearing accounts, and recovers in place", () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    render(<App />);
    expect(document.querySelector(".ring-value")?.textContent).toBe("72%");

    // A surface failure inside the boundary leaves the store untouched.
    function Boom(): React.ReactElement {
      throw new Error("surface failed");
    }
    const quiet = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const { container } = render(
      <FeatureBoundary surface="overview">
        <Boom />
      </FeatureBoundary>,
    );
    quiet.mockRestore();

    expect(container.textContent).toContain("The overview view stopped rendering");
    // Accounts are still present in the store, and no polling was restarted.
    expect(document.querySelector(".ring-value")?.textContent).toBe("72%");
    expect(commandsMatching("refresh_accounts")).toHaveLength(0);
  });
});

describe("the settings window route", () => {
  it("opens settings when the native window starts at #/settings", () => {
    window.location.hash = "#/settings";
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    render(<App />);
    expect(screen.getByRole("heading", { name: "Quota settings" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "General" })).toBeTruthy();
  });
});

describe("approved control actions", () => {
  it("saves only indicator style from either overview control", () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    render(<App />);
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Compact layout" }));
      fireEvent.click(screen.getByRole("button", { name: "Donut layout" }));
    });
    expect(commandsMatching("set_indicator_style").map((call) => call.args)).toEqual([
      { style: "bar" },
      { style: "ring" },
    ]);
    expect(commandsMatching("update_preferences")).toHaveLength(0);
  });

  it("keeps the filter after returning from details", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        ...oneAccount(),
        account("a2", "claude", 2, [quotaWindow("low", "session", percent(5))], {
          rank: 5,
        }),
      ]),
    );
    acceptPreferences(preferences());
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /^Attention/ }));
    expect(screen.getAllByRole("article")).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Details for Claude a2" }));
    fireEvent.click(screen.getByRole("button", { name: "All subscriptions" }));
    expect(
      screen.getByRole("button", { name: /^Attention/ }).getAttribute("aria-pressed"),
    ).toBe("true");
    expect(screen.getAllByRole("article")).toHaveLength(1);
  });

  it("wires pin, hide, Add account, details, and settings actions", () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    render(<App />);
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Dock to the tray" }));
      fireEvent.click(screen.getByRole("button", { name: "Hide popover" }));
      fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    });
    // Adding an account happens in the settings window, on its add-account page.
    expect(screen.queryByRole("heading", { name: "Add a subscription" })).toBeNull();
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Details for Codex a1" }));
    });
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Provider usage page" }));
      fireEvent.click(screen.getByRole("button", { name: "Manage account" }));
      fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    });
    expect(commandsMatching("set_overview_mode")[0]?.args).toEqual({ mode: "tray" });
    expect(commandsMatching("native-close")).toHaveLength(1);
    expect(commandsMatching("open_provider_usage_page")[0]?.args).toEqual({
      providerId: "codex",
    });
    expect(commandsMatching("open_settings_window").map((call) => call.args)).toEqual([
      { destination: "connect" },
      { destination: "accounts" },
      { destination: "general" },
    ]);
  });

  it("opens settings on its add-account page from first launch", async () => {
    acceptSnapshot(snapshot("instance-1", 1, []));
    render(<App />);
    await act(() =>
      fireEvent.click(screen.getByRole("button", { name: "Add your first account" })),
    );
    expect(screen.queryByRole("heading", { name: "Add a subscription" })).toBeNull();
    expect(commandsMatching("open_settings_window").map((call) => call.args)).toEqual([
      { destination: "connect" },
    ]);
  });

  it("returns to the overview on Escape and hides the popover from the overview", () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Details for Codex a1" }));
    expect(screen.getByRole("button", { name: "All subscriptions" })).toBeTruthy();
    act(() => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    expect(screen.queryByRole("button", { name: "All subscriptions" })).toBeNull();
    expect(commandsMatching("native-close")).toHaveLength(0);
    act(() => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    expect(commandsMatching("native-close")).toHaveLength(1);
  });

  it("renders the dedicated settings header and six sections without refresh", () => {
    window.location.hash = "#/settings";
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    render(<App />);
    expect(screen.getByRole("heading", { name: "Quota settings" })).toBeTruthy();
    for (const name of [
      "General",
      "Accounts",
      "Appearance",
      "Notifications",
      "Privacy",
      "Diagnostics",
    ])
      expect(screen.getByRole("button", { name })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Refresh readings" })).toBeNull();
  });

  it("shows the account's windows as detail tabs with scope labels for extra windows", () => {
    const owner = account("a1", "codex", 1, [
      quotaWindow("s", "session", percent(72)),
      quotaWindow("d", "daily", percent(30), { label: "Daily scope" }),
      quotaWindow("c", "custom", percent(10), { label: "Model X" }),
    ]);
    acceptSnapshot(snapshot("instance-1", 1, [owner]));
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Details for Codex a1" }));
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      "5-hour",
      "Daily",
      "Model X",
    ]);
    expect(
      screen.getByRole("tab", { name: "5-hour" }).getAttribute("aria-selected"),
    ).toBe("true");
    fireEvent.keyDown(screen.getByRole("tab", { name: "5-hour" }), { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Daily" }).getAttribute("aria-selected")).toBe(
      "true",
    );
    expect(screen.getByRole("group", { name: "Daily: 30% remaining" })).toBeTruthy();
  });

  it("opens the chosen window's tab from its ring", () => {
    const owner = account("a1", "codex", 1, [
      quotaWindow("s", "session", percent(72)),
      quotaWindow("w", "weekly", percent(41)),
    ]);
    acceptSnapshot(snapshot("instance-1", 1, [owner]));
    render(<App />);
    fireEvent.click(
      screen.getByRole("button", {
        name: "Codex a1, Weekly: 41% remaining. Resets in 2h 0m",
      }),
    );
    expect(
      screen.getByRole("tab", { name: "Weekly" }).getAttribute("aria-selected"),
    ).toBe("true");
    expect(screen.getByText("59% used / 41% left")).toBeTruthy();
  });
});

describe("General settings controls", () => {
  it("registers launch at login with the system and shows only the confirmed state", async () => {
    loginItem.enabled = false;
    loginItem.refuse = false;
    window.location.hash = "#/settings";
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    const view = render(<App />);
    const launchAtLogin = (): HTMLElement =>
      screen.getByRole("switch", { name: "Launch at login" });
    await waitFor(() => {
      expect(launchAtLogin()).toHaveProperty("disabled", false);
    });
    expect(launchAtLogin().getAttribute("aria-checked")).toBe("false");
    fireEvent.click(launchAtLogin());
    await waitFor(() => {
      expect(launchAtLogin().getAttribute("aria-checked")).toBe("true");
    });
    expect(loginItem.enabled).toBe(true);
    fireEvent.click(launchAtLogin());
    await waitFor(() => {
      expect(launchAtLogin().getAttribute("aria-checked")).toBe("false");
    });
    // A refusal leaves the switch as the system reports it and says why.
    loginItem.refuse = true;
    fireEvent.click(launchAtLogin());
    await waitFor(() => {
      expect(view.container.querySelector(".toast.show")?.textContent).toBe(
        "Windows did not change the login item. Launch at login is unchanged.",
      );
    });
    expect(launchAtLogin().getAttribute("aria-checked")).toBe("false");
    expect(commandsMatching("update_preferences")).toHaveLength(0);
  });

  it("keeps the real monitoring action", async () => {
    window.location.hash = "#/settings";
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ launch_behavior: "restore_last_mode" }));
    render(<App />);
    for (const name of ["Use wide view", "Fit all accounts", "Reset position"])
      expect(screen.queryByRole("button", { name })).toBeNull();
    act(() => {
      fireEvent.click(screen.getByRole("switch", { name: "Pause monitoring" }));
    });
    expect(commandsMatching("set_monitoring_state")[0]?.args).toEqual({ paused: true });
    expect(
      screen
        .getByRole("switch", { name: "Pause monitoring" })
        .getAttribute("aria-checked"),
    ).toBe("false");
    act(() => {
      acceptMonitoring({ kind: "paused" });
    });
    await act(() =>
      fireEvent.click(screen.getByRole("switch", { name: "Pause monitoring" })),
    );
    expect(commandsMatching("set_monitoring_state")[1]?.args).toEqual({ paused: false });
  });

  it("offers Resume until monitoring is confirmed running", async () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount(), { kind: "paused" }));
    render(<App />);
    expect(screen.getByText(/Monitoring is paused/)).toBeTruthy();
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Resume" })));
    expect(commandsMatching("set_monitoring_state")[0]?.args).toEqual({ paused: false });
    expect(screen.getByRole("button", { name: "Resume" })).toBeTruthy();
    act(() => {
      acceptMonitoring({ kind: "running" });
    });
    expect(screen.queryByRole("button", { name: "Resume" })).toBeNull();
  });

  const fixed = {
    visible_seconds: 300,
    background_seconds: 300,
    battery_saver_seconds: 600,
    minimum_seconds: 120,
  };
  const strategies: readonly {
    strategy: PollingStrategy;
    requested: string;
    expected: PollingStrategy;
  }[] = [
    {
      strategy: { kind: "fixed_interval", settings: fixed },
      requested: "60",
      expected: {
        kind: "fixed_interval",
        settings: { ...fixed, visible_seconds: 120, background_seconds: 120 },
      },
    },
    {
      strategy: {
        kind: "boundary_aware",
        settings: { base: fixed, boundary_grace_seconds: 30, max_boundary_attempts: 2 },
      },
      requested: "900",
      expected: {
        kind: "boundary_aware",
        settings: {
          base: {
            ...fixed,
            visible_seconds: 900,
            background_seconds: 900,
            battery_saver_seconds: 900,
          },
          boundary_grace_seconds: 30,
          max_boundary_attempts: 2,
        },
      },
    },
    {
      strategy: {
        kind: "adaptive",
        settings: { minimum_seconds: 120, maximum_seconds: 600, step_seconds: 300 },
      },
      requested: "900",
      expected: {
        kind: "adaptive",
        settings: { minimum_seconds: 120, maximum_seconds: 600, step_seconds: 600 },
      },
    },
    {
      strategy: {
        kind: "event_assisted",
        settings: { minimum_seconds: 120, verification_seconds: 300 },
      },
      requested: "60",
      expected: {
        kind: "event_assisted",
        settings: { minimum_seconds: 120, verification_seconds: 120 },
      },
    },
  ];
  it.each(strategies)(
    "saves a bounded interval without replacing $strategy.kind",
    async ({ strategy, requested, expected }) => {
      window.location.hash = "#/settings";
      const policy: ProviderPollingPolicy = {
        provider_id: "codex",
        strategy,
        request_timeout_seconds: 30,
        helper_timeout_seconds: 20,
        backoff_minutes: [1, 5, 15],
        max_concurrent_remote_reads: 1,
        version: 1,
      };
      acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
      acceptPreferences(preferences({ polling: [policy] }));
      render(<App />);
      const select = screen.getByRole("combobox", { name: "Background refresh" });
      expect(select).toHaveProperty("value", "300");
      await act(() => fireEvent.change(select, { target: { value: requested } }));
      const saved = { ...policy, strategy: expected };
      expect(commandsMatching("set_polling_preferences")[0]?.args).toEqual({
        providerId: "codex",
        policy: saved,
      });
      expect(commandsMatching("update_preferences")).toHaveLength(0);
      expect(select).toHaveProperty("value", "300");
      act(() => {
        acceptPreferences(preferences({ revision: 8, polling: [saved] }));
      });
      expect(select).toHaveProperty(
        "value",
        requested === "900" && strategy.kind === "boundary_aware" ? "900" : "",
      );
    },
  );

  it("updates every provider policy when providers have different intervals", async () => {
    window.location.hash = "#/settings";
    const policy: ProviderPollingPolicy = {
      provider_id: "codex",
      strategy: { kind: "fixed_interval", settings: fixed },
      request_timeout_seconds: 30,
      helper_timeout_seconds: 20,
      backoff_minutes: [1, 5],
      max_concurrent_remote_reads: 1,
      version: 1,
    };
    const sibling: ProviderPollingPolicy = {
      ...policy,
      provider_id: "claude",
      strategy: {
        kind: "event_assisted",
        settings: { minimum_seconds: 60, verification_seconds: 900 },
      },
    };
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ polling: [policy, sibling] }));
    render(<App />);
    const select = screen.getByRole("combobox", { name: "Background refresh" });
    expect(select).toHaveProperty("value", "");
    await act(() => fireEvent.change(select, { target: { value: "300" } }));
    expect(commandsMatching("set_polling_preferences").map((call) => call.args)).toEqual([
      { providerId: "codex", policy },
      {
        providerId: "claude",
        policy: {
          ...sibling,
          strategy: {
            kind: "event_assisted",
            settings: { minimum_seconds: 60, verification_seconds: 300 },
          },
        },
      },
    ]);
  });
});

describe("the Fit transition", () => {
  it("returns to all accounts and the overview when the host completes Fit", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        ...oneAccount(),
        account("a2", "claude", 2, [quotaWindow("low", "session", percent(5))], {
          rank: 5,
        }),
      ]),
    );
    acceptPreferences(preferences({ overview_mode: "tray" }));
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /^Attention/ }));
    expect(screen.getAllByRole("article")).toHaveLength(1);
    act(() => {
      window.dispatchEvent(new Event("quota-fit-overview"));
    });
    expect(screen.getAllByRole("article")).toHaveLength(2);
    expect(
      screen.getByRole("button", { name: /^All accounts/ }).getAttribute("aria-pressed"),
    ).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Details for Claude a2" }));
    expect(screen.queryAllByRole("article")).toHaveLength(0);
    act(() => {
      window.dispatchEvent(new Event("quota-fit-overview"));
    });
    expect(screen.getAllByRole("article")).toHaveLength(2);
  });
});

describe("account details privacy", () => {
  /** An account whose every label is private. */
  function privateOwner(): ReturnType<typeof account> {
    const owner = account(
      "a2",
      "claude",
      2,
      [
        quotaWindow("weekly-opus", "weekly", percent(72), { label: "Claude Opus" }),
        quotaWindow("weekly-sonnet", "weekly", percent(30), { label: "Claude Sonnet" }),
        quotaWindow("custom", "custom", percent(40), { label: "Extra usage" }),
        quotaWindow("daily", "daily", percent(50), { label: "Daily credits" }),
      ],
      { nickname: "Private nickname", rank: 72 },
    );
    owner.identity = {
      principal_label: "private@example.test",
      workspace_label: "Private workspace",
      plan_label: "Max",
      source: "documented_api",
    };
    return owner;
  }
  const hidden = preferences({
    privacy: { ...preferences().privacy, alias_mode: "stable_aliases" },
  });

  it("asks the popover to show details from the settings entry point", async () => {
    window.location.hash = "#/settings/accounts";
    acceptSnapshot(
      snapshot("instance-1", 1, [privateOwner(), account("a1", "claude", 1, [])]),
    );
    acceptPreferences(hidden);
    render(<App />);
    const card = screen.getByRole("article", { name: "Manage Claude Account 2" });
    for (const privateLabel of [
      "Private nickname",
      "Private workspace",
      "private@example.test",
    ])
      expect(card.textContent).not.toContain(privateLabel);
    expect(within(card).getByRole("button", { name: "Rename" })).toHaveProperty(
      "disabled",
      true,
    );
    await act(() =>
      fireEvent.click(within(card).getByRole("button", { name: "Details" })),
    );
    await waitFor(() => {
      expect(commandsMatching("native-close")).toHaveLength(1);
    });
    expect(commandsMatching("emit-to")[0]?.args).toEqual({
      target: "overview",
      event: "quota-popover-navigate",
      payload: { view: "detail", accountId: "a2", windowId: null },
    });
  });

  it("aliases every identity in the popover detail and responds to confirmed changes", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [privateOwner(), account("a1", "claude", 1, [])]),
    );
    acceptPreferences(hidden);
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Details for Claude Account 2" }));
    const details = screen.getByRole("region", { name: "Quota detail for Account 2" });
    for (const privateLabel of [
      "Private nickname",
      "private@example.test",
      "Private workspace",
    ])
      expect(details.outerHTML).not.toContain(privateLabel);
    expect(within(details).getByText("Account 2")).toBeTruthy();
    expect(
      within(details)
        .getAllByRole("tab")
        .map((tab) => tab.textContent),
    ).toEqual(["Daily", "Weekly", "Claude Sonnet", "Extra usage"]);
    fireEvent.click(within(details).getByRole("tab", { name: "Weekly" }));
    expect(details.textContent).toContain("Claude Opus");
    expect(details.textContent).toContain("28% used / 72% left");
    expect(details.textContent).toContain("Documented API");
    fireEvent.click(
      within(details).getByRole("button", { name: /Claude Sonnet.*30% remaining/ }),
    );
    expect(
      within(details)
        .getByRole("tab", { name: "Claude Sonnet" })
        .getAttribute("aria-selected"),
    ).toBe("true");
    expect(details.textContent).toContain("70% used / 30% left");

    act(() => {
      acceptPreferences(preferences({ revision: hidden.revision + 1 }));
    });
    const shown = screen.getByRole("region", {
      name: "Quota detail for Private nickname",
    });
    expect(within(shown).getByText("Private nickname")).toBeTruthy();
    act(() => {
      acceptPreferences({ ...hidden, revision: hidden.revision + 2 });
    });
    const again = screen.getByRole("region", { name: "Quota detail for Account 2" });
    expect(again.outerHTML).not.toContain("Private nickname");
    // The chosen tab survives a label change.
    expect(
      within(again)
        .getByRole("tab", { name: "Claude Sonnet" })
        .getAttribute("aria-selected"),
    ).toBe("true");
  });
});

describe("toasts", () => {
  it("states a refused command where the person acted", async () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    const view = render(<App />);
    act(() => {
      setFailure({ kind: "domain", error: { kind: "initialization_pending" } });
    });
    await waitFor(() => {
      expect(view.container.querySelector(".toast.show")?.textContent).toBe(
        "Quota is still starting up.",
      );
    });
  });

  it("answers the refresh button with a toast", async () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    const view = render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Refresh readings" }));
    await waitFor(() => {
      expect(view.container.querySelector(".toast.show")?.textContent).toBe(
        "Refreshing readings.",
      );
    });
    expect(commandsMatching("refresh_accounts")).toHaveLength(1);
  });
});
