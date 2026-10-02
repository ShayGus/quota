/**
 * Theme, always-on-top, and boundary behaviour.
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
    return Promise.resolve(null);
  },
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    close: () => {
      invoked.push({ command: "native-close", args: null });
      return Promise.resolve();
    },
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

// `vi.mock` is hoisted above these imports by the test runner, so the transport
// is replaced before the module graph under test is evaluated.
import { App } from "../src/app/App";
import { FeatureBoundary } from "../src/app/ErrorBoundary";
import {
  acceptMonitoring,
  acceptPreferences,
  acceptSnapshot,
  applyPendingOrder,
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

describe("the always-on-top control", () => {
  it("sends only the always-on-top preference and nothing else", async () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ always_on_top: false }));
    render(<App />);

    const pin = screen.getByRole("button", { name: /keep the window on top/i });
    await act(async () => {
      pin.click();
      // The command is issued from the handler and settles on a microtask.
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(commandsMatching("set_overview_always_on_top")).toHaveLength(1);
    });
    // Tauri names command arguments after the Rust parameter, so the request
    // struct travels under the `request` key.
    expect(commandsMatching("set_overview_always_on_top")[0]?.args).toEqual({
      alwaysOnTop: true,
    });
    // The pin sends nothing else. The only other call is the one-time snapshot
    // reconciliation every window performs at mount.
    expect(mutatingCommands()).toEqual(["set_overview_always_on_top"]);
  });

  it("shows the selected state in words as well as in colour", () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ always_on_top: true }));
    render(<App />);

    expect(screen.getByText("Always on top")).toBeTruthy();
    const pin = screen.getByRole("button", { name: /turn off always on top/i });
    expect(pin.getAttribute("aria-pressed")).toBe("true");
  });
});

describe("a render failure", () => {
  it("contains the failure without clearing accounts, and recovers in place", () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    render(<App />);
    expect(screen.getAllByText("72%").length).toBeGreaterThan(0);

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
    expect(screen.getAllByText("72%").length).toBeGreaterThan(0);
    expect(commandsMatching("refresh_accounts")).toHaveLength(0);
  });
});

describe("the settings window route", () => {
  it("opens settings when the native window starts at #/settings", () => {
    window.location.hash = "#/settings";
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    render(<App />);
    expect(screen.getByLabelText("Quota settings")).toBeTruthy();
  });
});

describe("approved control actions", () => {
  it("saves only indicator style from either overview control", () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    render(<App />);
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Bar indicators" }));
      fireEvent.click(screen.getByRole("button", { name: "Ring indicators" }));
    });
    expect(commandsMatching("set_indicator_style").map((call) => call.args)).toEqual([
      { style: "bar" },
      { style: "ring" },
    ]);
    expect(commandsMatching("update_preferences")).toHaveLength(0);
  });

  it("keeps search visible and active after returning from details", () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Find an account" }));
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "a1" } });
    fireEvent.click(screen.getByRole("button", { name: "Details for a1" }));
    fireEvent.click(screen.getByRole("button", { name: "All accounts" }));
    expect(screen.getByRole("searchbox")).toHaveProperty("value", "a1");
    fireEvent.click(screen.getByRole("button", { name: "Find an account" }));
    expect(screen.queryByRole("searchbox")).toBeNull();
    expect(screen.getByRole("article")).toBeTruthy();
  });

  it("wires mode, hide, Add account, and detail actions", () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    render(<App />);
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Switch to tray popover" }));
      fireEvent.click(screen.getByRole("button", { name: "Hide Quota to tray" }));
      fireEvent.click(screen.getByRole("button", { name: "Add account" }));
      fireEvent.click(screen.getByRole("button", { name: "Details for a1" }));
    });
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Provider usage page" }));
      fireEvent.click(screen.getByRole("button", { name: "Manage accounts" }));
      fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
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

  it("opens the same wizard from first launch", async () => {
    acceptSnapshot(snapshot("instance-1", 1, []));
    render(<App />);
    await act(() =>
      fireEvent.click(screen.getByRole("button", { name: "Add your first account" })),
    );
    expect(commandsMatching("open_settings_window")[0]?.args).toEqual({
      destination: "connect",
    });
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
    expect(screen.queryByRole("button", { name: "Refresh the readings now" })).toBeNull();
    expect(screen.queryByTestId("visible-count")).toBeNull();
  });

  it("uses standard titles and scope labels for extra windows", () => {
    const owner = account("a1", "codex", 1, [
      quotaWindow("s", "session", percent(72)),
      quotaWindow("d", "daily", percent(30), { label: "Daily scope" }),
      quotaWindow("c", "custom", percent(10), { label: "Model X" }),
    ]);
    acceptSnapshot(snapshot("instance-1", 1, [owner]));
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Details for a1" }));
    for (const name of ["5-hour", "Daily scope", "Model X"])
      expect(screen.getByRole("heading", { name })).toBeTruthy();
  });

  it("counts fully visible filtered rows on scroll and resize", () => {
    let bottom = 200;
    let offset = 0;
    const rect = (top: number, end: number): DOMRect => ({
      top,
      bottom: end,
      left: 0,
      right: 810,
      width: 810,
      height: end - top,
      x: 0,
      y: top,
      toJSON: () => ({}),
    });
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
      this: HTMLElement,
    ) {
      if (this.classList.contains("shell__main")) return rect(0, bottom);
      if (this.classList.contains("table__columns")) return rect(0, 26);
      const id = this.dataset["accountId"];
      if (id !== undefined) {
        const top = Number(id.slice(1)) * 60 - offset;
        return rect(top, top + 54);
      }
      return rect(0, 0);
    });
    acceptSnapshot(
      snapshot(
        "instance-1",
        1,
        [1, 2, 3, 4].map((ordinal) =>
          account(`a${ordinal}`, "codex", ordinal, [], { rank: ordinal }),
        ),
      ),
    );
    render(<App />);
    expect(screen.getByTestId("visible-count").textContent).toBe("2 / 4 visible");
    offset = 60;
    const list = document.querySelector(".shell__main");
    if (list === null) {
      throw new Error("the account list must be on screen");
    }
    fireEvent.scroll(list);
    expect(screen.getByTestId("visible-count").textContent).toBe("2 / 4 visible");
    bottom = 280;
    fireEvent(window, new Event("resize"));
    expect(screen.getByTestId("visible-count").textContent).toBe("3 / 4 visible");
    fireEvent.click(screen.getByRole("button", { name: "Find an account" }));
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "a3" } });
    expect(screen.getByTestId("visible-count").textContent).toBe("1 / 1 visible");
  });
});

describe("General settings controls", () => {
  it("disables unavailable operations and preserves the real geometry and monitoring actions", async () => {
    window.location.hash = "#/settings";
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ launch_behavior: "restore_last_mode" }));
    render(<App />);
    for (const [role, name] of [
      ["button", "Try narrow view"],
      ["button", "Move with keys"],
      ["switch", "Launch at login"],
    ] as const) {
      const control = screen.getByRole(role, { name });
      expect(control).toHaveProperty("disabled", true);
      fireEvent.click(control);
    }
    expect(commandsMatching("reset_overview_position")).toHaveLength(0);
    expect(commandsMatching("update_preferences")).toHaveLength(0);
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Use wide view" }));
      fireEvent.click(screen.getByRole("button", { name: "Fit all accounts" }));
      fireEvent.click(screen.getByRole("button", { name: "Reset position" }));
      fireEvent.click(screen.getByRole("switch", { name: "Pause monitoring" }));
    });
    expect(commandsMatching("fit_overview_to_accounts")).toHaveLength(2);
    expect(commandsMatching("reset_overview_position")).toHaveLength(1);
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

describe("Fit presentation and layout changes", () => {
  it("restores all accounts and closes search when the host completes Fit", async () => {
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
    fireEvent.click(screen.getByRole("button", { name: /Attention/ }));
    fireEvent.click(screen.getByRole("button", { name: "Find an account" }));
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "a2" } });
    expect(screen.getAllByRole("article")).toHaveLength(1);
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Fit 2" })));
    expect(commandsMatching("fit_overview_to_accounts")).toHaveLength(1);
    expect(screen.getAllByRole("article")).toHaveLength(1);
    act(() => {
      window.dispatchEvent(new Event("quota-fit-overview"));
    });
    expect(screen.getAllByRole("article")).toHaveLength(2);
    expect(screen.queryByRole("searchbox")).toBeNull();
    expect(
      screen.getByRole("button", { name: /^All accounts/ }).getAttribute("aria-pressed"),
    ).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Find an account" }));
    expect(screen.getByRole("searchbox")).toHaveProperty("value", "");
    fireEvent.click(screen.getByRole("button", { name: "Details for a2" }));
    act(() => {
      window.dispatchEvent(new Event("quota-fit-overview"));
    });
    expect(screen.getAllByRole("article")).toHaveLength(2);
    expect(screen.queryByRole("button", { name: "All accounts" })).toBeNull();
  });

  it("recounts help opening and both closing paths while viewport and row sizes stay fixed", () => {
    const observers: { targets: Set<Element>; notify: () => void }[] = [];
    vi.stubGlobal(
      "ResizeObserver",
      class implements ResizeObserver {
        readonly targets = new Set<Element>();
        constructor(callback: ResizeObserverCallback) {
          observers.push({
            targets: this.targets,
            notify: () => {
              callback([], this);
            },
          });
        }
        observe(target: Element): void {
          this.targets.add(target);
        }
        unobserve(target: Element): void {
          this.targets.delete(target);
        }
        disconnect(): void {
          this.targets.clear();
        }
      },
    );
    const rect = (top: number, bottom: number): DOMRect => ({
      top,
      bottom,
      left: 0,
      right: 810,
      width: 810,
      height: bottom - top,
      x: 0,
      y: top,
      toJSON: () => ({}),
    });
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
      this: HTMLElement,
    ) {
      const shift =
        screen.queryByRole("button", { name: "Close ordering help" }) === null ? 0 : 80;
      if (this.classList.contains("shell__main")) return rect(0, 200);
      if (this.classList.contains("table__columns")) return rect(0, 26);
      if (this.classList.contains("overview")) return rect(0, 400 + shift);
      const id = this.dataset["accountId"];
      if (id !== undefined) {
        const top = Number(id.slice(1)) * 60 + shift;
        return rect(top, top + 54);
      }
      return rect(0, 0);
    });
    acceptSnapshot(
      snapshot(
        "instance-1",
        1,
        [1, 2, 3, 4].map((ordinal) =>
          account(`a${ordinal}`, "codex", ordinal, [], { rank: ordinal }),
        ),
      ),
    );
    applyPendingOrder();
    render(<App />);
    const content = document.querySelector(".overview");
    if (content === null) {
      throw new Error("the overview must be on screen");
    }
    const resized = (): void => {
      for (const observer of observers)
        if (observer.targets.has(content)) observer.notify();
    };
    expect(screen.getByTestId("visible-count").textContent).toBe("2 / 4 visible");
    fireEvent.click(screen.getByRole("button", { name: "Least remaining first" }));
    act(resized);
    expect(screen.getByTestId("visible-count").textContent).toBe("1 / 4 visible");
    fireEvent.click(screen.getByRole("button", { name: "Least remaining first" }));
    act(resized);
    expect(screen.getByTestId("visible-count").textContent).toBe("2 / 4 visible");
    fireEvent.click(screen.getByRole("button", { name: "Least remaining first" }));
    act(resized);
    expect(screen.getByTestId("visible-count").textContent).toBe("1 / 4 visible");
    fireEvent.click(screen.getByRole("button", { name: "Close ordering help" }));
    act(resized);
    expect(screen.getByTestId("visible-count").textContent).toBe("2 / 4 visible");
  });
});

describe("account details privacy", () => {
  it.each(["overview", "settings"])(
    "aliases every identity from the %s entry point and responds to confirmed changes",
    (entry) => {
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
      const base = preferences();
      const hidden = preferences({
        privacy: { ...base.privacy, alias_mode: "stable_aliases" },
      });
      window.location.hash = entry === "settings" ? "#/settings/accounts" : "";
      acceptSnapshot(snapshot("instance-1", 1, [owner, account("a1", "claude", 1, [])]));
      acceptPreferences(hidden);
      render(<App />);
      if (entry === "settings") {
        const card = screen.getByRole("article", { name: "Manage Account 2" });
        expect(card.textContent).not.toContain("Private nickname");
        expect(card.textContent).not.toContain("Private workspace");
        expect(card.textContent).not.toContain("private@example.test");
        expect(within(card).getByRole("button", { name: "Rename" })).toHaveProperty(
          "disabled",
          true,
        );
        fireEvent.click(within(card).getByRole("button", { name: "Details" }));
      } else {
        fireEvent.click(screen.getByRole("button", { name: "Details for Account 2" }));
      }
      const details = screen.getByRole("region", {
        name: "Account details for Account 2",
      });
      for (const privateLabel of [
        "Private nickname",
        "private@example.test",
        "Private workspace",
      ])
        expect(details.outerHTML).not.toContain(privateLabel);
      expect(within(details).getByRole("heading", { name: "Account 2" })).toBeTruthy();
      expect(details.textContent).toContain("Workspace hidden");
      expect(details.textContent).toContain("72%");
      expect(details.textContent).toContain("Documented API");
      expect(
        within(details).getByRole("button", { name: /Claude Opus: 72%/ }),
      ).toBeTruthy();
      expect(within(details).getByText("Claude Opus")).toBeTruthy();
      expect(within(details).getByText("Claude Sonnet")).toBeTruthy();
      expect(details.textContent).toContain("Documented API · Claude Opus");
      expect(details.textContent).toContain("Ranked by Claude Opus: 72%");
      expect(within(details).getByRole("heading", { name: "Extra usage" })).toBeTruthy();
      expect(
        within(details).getByRole("heading", { name: "Daily credits" }),
      ).toBeTruthy();
      expect(
        within(details).getByRole("button", { name: /Extra usage: 40%/ }),
      ).toBeTruthy();
      expect(
        within(details).getByRole("button", { name: /Daily credits: 50%/ }),
      ).toBeTruthy();
      fireEvent.click(
        within(details).getByRole("button", { name: /Claude Sonnet: 30%/ }),
      );
      expect(details.textContent).toContain("Claude Sonnet boundary");
      expect(details.textContent).toContain("Documented API · Claude Sonnet");

      act(() => {
        acceptPreferences(preferences({ revision: hidden.revision + 1 }));
      });
      expect(
        within(details).getByRole("heading", { name: "Private nickname" }),
      ).toBeTruthy();
      expect(details.textContent).toContain("private@example.test");
      expect(details.textContent).toContain("Private workspace");
      expect(details.getAttribute("aria-label")).toBe(
        "Account details for Private nickname",
      );
      act(() => {
        acceptPreferences({ ...hidden, revision: hidden.revision + 2 });
      });
      for (const privateLabel of [
        "Private nickname",
        "private@example.test",
        "Private workspace",
      ])
        expect(details.outerHTML).not.toContain(privateLabel);
      expect(
        within(details)
          .getByRole("button", { name: /Claude Sonnet: 30%/ })
          .getAttribute("aria-pressed"),
      ).toBe("true");
    },
  );
});
