import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import type { AttemptRef } from "../src/generated/bindings";
import { ConnectionWizard } from "../src/features/settings/ConnectionWizard";
import { Settings, type SettingsActions } from "../src/features/settings/Settings";
import { Overview } from "../src/features/overview/Overview";
import { initialRendererState } from "../src/shared/state/types";
import { refreshMessage, Toast, TOAST_MS } from "../src/shared/ui/RefreshNotice";
import {
  account,
  awaitingConfirmation,
  candidate,
  preferences,
  snapshot,
} from "./fixtures";

function settingsActions(): SettingsActions {
  return {
    savePreferences: vi.fn(),
    setMonitoring: vi.fn(),
    savePollingPreferences: vi.fn(),
    setAlwaysOnTop: vi.fn(),
    setOverviewMode: vi.fn(),
    setAccountEnabled: vi.fn(),
    renameAccount: vi.fn(),
    disconnectAccount: vi.fn(),
    openUsagePage: vi.fn(),
    beginConnection: vi.fn(() => Promise.resolve(null)),
    cancelConnection: vi.fn(() => Promise.resolve()),
    confirmConnection: vi.fn(() => Promise.resolve(true)),
    reconnectAccount: vi.fn(() => Promise.resolve()),
    clearHistory: vi.fn(),
    exportDiagnostics: vi.fn(() =>
      Promise.resolve("/data/diagnostics/quota-diagnostics-settings.json"),
    ),
    showOverview: vi.fn(),
    showAccountDetail: vi.fn(),
    launchAtLogin: vi.fn(() => Promise.resolve(false)),
    setLaunchAtLogin: vi.fn((launch: boolean) => Promise.resolve(launch)),
  };
}

describe("pending connection acceptance", () => {
  it("blocks duplicate submissions and stays busy until terminal progress", async () => {
    const pending = Promise.withResolvers<AttemptRef | null>();
    const beginConnection = vi.fn(() => pending.promise);
    const actions = { ...settingsActions(), beginConnection };
    const panel = render(
      <ConnectionWizard
        state={{ ...initialRendererState, preferences: preferences() }}
        actions={actions}
        onDone={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /^Codex/ }));
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));

    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Verifying…" }).disabled,
    ).toBe(true);
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Back" }).disabled).toBe(
      true,
    );
    fireEvent.click(screen.getByRole("button", { name: "Verifying…" }));
    expect(beginConnection).toHaveBeenCalledTimes(1);
    expect(beginConnection).toHaveBeenCalledWith({
      provider_id: "codex",
      nickname: "Personal",
      profile_label: null,
    });

    await act(async () => {
      pending.resolve({ id: "attempt" });
      await pending.promise;
    });
    expect(screen.getByRole("button", { name: "Verifying…" })).toHaveProperty(
      "disabled",
      true,
    );
    panel.rerender(
      <ConnectionWizard
        state={{
          ...initialRendererState,
          preferences: preferences(),
          attempts: [
            {
              attemptId: "attempt",
              revision: 1,
              progress: { kind: "awaiting_user" },
            },
          ],
        }}
        actions={actions}
        onDone={vi.fn()}
      />,
    );
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Verifying…" }).disabled,
    ).toBe(true);
    // Verified: the host holds the candidate and nothing is saved yet.
    panel.rerender(
      <ConnectionWizard
        state={{
          ...initialRendererState,
          preferences: preferences(),
          attempts: [awaitingConfirmation("attempt", candidate("codex"))],
        }}
        actions={actions}
        onDone={vi.fn()}
      />,
    );
    expect(screen.queryByRole("button", { name: "Verifying…" })).toBeNull();
    expect(screen.getByRole("heading", { name: "Add this account?" })).toBeDefined();
    expect(screen.getByRole("button", { name: "Add Codex account" })).toHaveProperty(
      "disabled",
      false,
    );
  });

  it("releases the controls after acceptance is refused", async () => {
    const actions = settingsActions();
    render(
      <ConnectionWizard
        state={{ ...initialRendererState, preferences: preferences() }}
        actions={actions}
        onDone={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /^Codex/ }));
    await act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Connect" }));
      return Promise.resolve();
    });
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Connect" }).disabled,
    ).toBe(false);
    expect(screen.getByRole("button", { name: "Back" })).toHaveProperty(
      "disabled",
      false,
    );
    expect(screen.getByRole("alert").textContent).toContain("connection was refused");
  });
});

describe("settings connection session", () => {
  it("opens its own add-account page from Accounts", () => {
    window.history.replaceState(null, "", "#/settings/accounts");
    const actions = settingsActions();
    const state = { ...initialRendererState, preferences: preferences() };
    const view = render(<Settings state={state} actions={actions} />);
    // Both the title action and the empty state's action open the add-account page.
    const adds = screen.getAllByRole("button", { name: "Add account" });
    expect(adds).toHaveLength(2);
    const [add] = adds;
    if (add === undefined) throw new Error("Add account is missing");
    act(() => {
      fireEvent.click(add);
      fireEvent(window, new HashChangeEvent("hashchange"));
    });
    expect(window.location.hash).toMatch(/^#\/settings\/connect\//);
    view.rerender(<Settings state={state} actions={actions} />);
    expect(screen.getByRole("heading", { name: "Add a subscription" })).toBeTruthy();
  });

  it("keeps acceptance and provider recovery on the host's connection route", async () => {
    window.history.replaceState(null, "", "#/settings/connect");
    const pending = Promise.withResolvers<AttemptRef | null>();
    const beginConnection = vi.fn(() => pending.promise);
    const actions = { ...settingsActions(), beginConnection };
    const state = { ...initialRendererState, preferences: preferences() };
    const settings = render(<Settings state={state} actions={actions} />);
    expect(screen.getByRole("heading", { name: "Add a subscription" })).toBeDefined();
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    settings.rerender(<Settings state={{ ...state, link: "live" }} actions={actions} />);
    expect(screen.getByRole("button", { name: "Verifying…" })).toHaveProperty(
      "disabled",
      true,
    );
    expect(beginConnection).toHaveBeenCalledTimes(1);
    await act(async () => {
      pending.resolve({ id: "attempt" });
      await pending.promise;
    });
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Verifying…" }).disabled,
    ).toBe(true);
    settings.rerender(
      <Settings
        state={{
          ...state,
          attempts: [
            {
              attemptId: "attempt",
              revision: 2,
              progress: {
                kind: "failed",
                context: { error: { kind: "reconnect_required" } },
              },
            },
          ],
        }}
        actions={actions}
      />,
    );
    expect(screen.getByRole("alert").textContent).toContain("Run claude in a terminal");
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Connect" }).disabled,
    ).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Accounts" })).toBeDefined();
    });
    expect(actions.cancelConnection).not.toHaveBeenCalled();
  });
});

describe("refresh timing", () => {
  const now = Date.parse("2026-10-01T12:00:00.000Z");
  const waiting = {
    ...account("a1", "codex", 1, []),
    next_attempt_at: "2026-10-01T12:05:00.000Z",
  };

  it("states a deferred manual refresh when it is requested, not as a standing banner", () => {
    expect(refreshMessage([waiting], preferences(), now)).toContain(
      "Next eligible read in 5m.",
    );
    expect(refreshMessage([waiting], preferences(), now + 300_000)).toBe(
      "Refreshing readings.",
    );
    render(
      <Overview
        state={{
          ...initialRendererState,
          snapshot: snapshot("instance-1", 1, [waiting]),
        }}
        filter="all"
        onFilter={vi.fn()}
        onAddAccount={vi.fn()}
        onIndicatorStyle={vi.fn()}
        onResume={vi.fn()}
        onOpenAccount={() => undefined}
        onOpenWindow={() => undefined}
        onReconnect={() => undefined}
        onEnable={() => undefined}
      />,
    );
    expect(screen.queryByText(/Manual refreshes.*are deferred/)).toBeNull();
  });

  it("shows a toast and hides it again after a moment", () => {
    vi.useFakeTimers();
    try {
      const toast = render(<Toast message={null} />);
      expect(toast.container.querySelector(".toast.show")).toBeNull();
      toast.rerender(<Toast message={{ text: "Refreshing readings." }} />);
      expect(toast.container.querySelector(".toast.show")?.textContent).toBe(
        "Refreshing readings.",
      );
      act(() => {
        vi.advanceTimersByTime(TOAST_MS);
      });
      expect(toast.container.querySelector(".toast.show")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it.each([
    ["stable_aliases", "Account 2"],
    ["off", "Captain's account"],
  ] as const)("names the account with the %s label", (mode, label) => {
    const named = {
      ...account("z", "codex", 1, [], { nickname: "Captain's account" }),
      next_attempt_at: "2026-10-01T12:05:00.000Z",
    };
    const confirmed = preferences();
    const message = refreshMessage(
      [named, account("a", "claude", 2, [])],
      preferences({ privacy: { ...confirmed.privacy, alias_mode: mode } }),
      now,
    );
    expect(message).toContain(`Manual refreshes for ${label} are deferred.`);
    if (mode === "stable_aliases") {
      expect(message).not.toContain(named.nickname);
    }
  });

  it.each([
    {
      name: "short",
      nextAttempt: "2026-10-01T12:05:00.000Z",
      initial: "5m",
      later: "4m",
    },
    {
      name: "long",
      nextAttempt: "2026-10-01T13:00:00.000Z",
      initial: "1h 0m",
      later: "59m",
    },
  ])("formats the snapshot deadline after $name Retry-After", (reading) => {
    const backoff = {
      ...account("a1", "codex", 1, []),
      fetch_state: "backoff" as const,
      last_attempt_at: "2026-10-01T12:00:00.000Z",
      next_attempt_at: reading.nextAttempt,
    };
    const first = refreshMessage([backoff], null, now);
    expect(first).toContain(`Next eligible read in ${reading.initial}.`);
    expect(first).not.toContain(reading.nextAttempt);
    expect(refreshMessage([backoff], null, now + 60_000)).toContain(
      `Next eligible read in ${reading.later}.`,
    );
  });
});
