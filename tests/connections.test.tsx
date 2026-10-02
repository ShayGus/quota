import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import type { AttemptRef } from "../src/generated/bindings";
import { ConnectionWizard } from "../src/features/settings/ConnectionWizard";
import { AccountsPanel } from "../src/features/settings/panels/AccountsPanel";
import { Settings, type SettingsActions } from "../src/features/settings/Settings";
import { Overview } from "../src/features/overview/Overview";
import { initialRendererState } from "../src/shared/state/types";
import { RefreshNotice } from "../src/shared/ui/RefreshNotice";
import { account, preferences, snapshot } from "./fixtures";

function settingsActions(): SettingsActions {
  return {
    savePreferences: vi.fn(),
    setMonitoring: vi.fn(),
    savePollingPreferences: vi.fn(),
    setAlwaysOnTop: vi.fn(),
    setOverviewMode: vi.fn(),
    fitToAccounts: vi.fn(),
    resetPosition: vi.fn(),
    setAccountEnabled: vi.fn(),
    renameAccount: vi.fn(),
    disconnectAccount: vi.fn(),
    openUsagePage: vi.fn(),
    beginConnection: vi.fn(() => Promise.resolve(null)),
    cancelConnection: vi.fn(() => Promise.resolve()),
    reconnectAccount: vi.fn(() => Promise.resolve()),
    clearHistory: vi.fn(),
    exportDiagnostics: vi.fn(),
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
    expect(screen.getByLabelText<HTMLInputElement>("Account nickname").disabled).toBe(
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
    panel.rerender(
      <ConnectionWizard
        state={{
          ...initialRendererState,
          preferences: preferences(),
          attempts: [
            {
              attemptId: "attempt",
              revision: 2,
              progress: { kind: "verified", context: { state: "connected" } },
            },
          ],
        }}
        actions={actions}
        onDone={vi.fn()}
      />,
    );
    expect(screen.queryByRole("button", { name: "Verifying…" })).toBeNull();
    expect(screen.getByRole("heading", { name: "Verify your connection" })).toBeDefined();
    expect(screen.getByRole("button", { name: "Manage accounts" })).toHaveProperty(
      "disabled",
      true,
    );
    fireEvent.click(screen.getByRole("checkbox"));
    expect(screen.getByRole("button", { name: "Manage accounts" })).toHaveProperty(
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
    expect(screen.getByLabelText("Account nickname")).toHaveProperty("disabled", false);
    expect(screen.getByRole("button", { name: "Back" })).toHaveProperty(
      "disabled",
      false,
    );
    expect(screen.getByRole("alert").textContent).toContain("connection was refused");
  });
});

describe("settings connection session", () => {
  it("opens the wizard from Accounts and keeps acceptance and provider recovery on its route", async () => {
    window.history.replaceState(null, "", "#/settings/accounts");
    const pending = Promise.withResolvers<AttemptRef | null>();
    const beginConnection = vi.fn(() => pending.promise);
    const actions = { ...settingsActions(), beginConnection };
    const state = { ...initialRendererState, preferences: preferences() };
    const settings = render(<Settings state={state} actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Add a subscription" })).toBeDefined();
    });
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
      expect(screen.getByRole("button", { name: "Add account" })).toBeDefined();
    });
    expect(actions.cancelConnection).not.toHaveBeenCalled();
  });
});

describe("snapshot refresh timing", () => {
  const now = Date.parse("2026-10-01T12:00:00.000Z");
  const waiting = {
    ...account("a1", "codex", 1, []),
    next_attempt_at: "2026-10-01T12:05:00.000Z",
  };

  it("shows deferred manual refreshes in settings", () => {
    const panel = render(
      <AccountsPanel
        accounts={[waiting]}
        preferences={preferences()}
        now={now}
        actions={settingsActions()}
        onAddAccount={vi.fn()}
      />,
    );
    expect(screen.getByText(/Manual refreshes.*are deferred/).textContent).toContain(
      "5m",
    );
    panel.rerender(
      <AccountsPanel
        accounts={[waiting]}
        preferences={preferences()}
        now={now + 300_000}
        actions={settingsActions()}
        onAddAccount={vi.fn()}
      />,
    );
    expect(screen.queryByText(/Manual refreshes.*are deferred/)).toBeNull();
  });

  it("shows the same timing in the overview", () => {
    vi.useFakeTimers();
    vi.setSystemTime(now);
    try {
      render(
        <Overview
          state={{
            ...initialRendererState,
            snapshot: snapshot("instance-1", 1, [waiting]),
          }}
          filter="all"
          onFilter={vi.fn()}
          search=""
          onSearch={vi.fn()}
          searchOpen={false}
          onSearchOpen={vi.fn()}
          onFit={vi.fn()}
          onAddAccount={vi.fn()}
          onIndicatorStyle={vi.fn()}
          onResume={vi.fn()}
          onOpenAccount={() => undefined}
          onReconnect={() => undefined}
        />,
      );
      expect(screen.getByText(/Manual refreshes.*are deferred/).textContent).toContain(
        "5m",
      );
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("refresh notice privacy", () => {
  it.each([
    ["stable_aliases", "Account 2"],
    ["off", "Captain's account"],
  ] as const)("uses the same %s label in overview and settings", (mode, label) => {
    vi.useFakeTimers();
    vi.setSystemTime("2026-10-01T12:00:00.000Z");
    const waiting = {
      ...account("z", "codex", 1, [], { nickname: "Captain's account" }),
      next_attempt_at: "2026-10-01T12:05:00.000Z",
    };
    const confirmed = preferences();
    const state = {
      ...initialRendererState,
      preferences: preferences({ privacy: { ...confirmed.privacy, alias_mode: mode } }),
      snapshot: snapshot("instance-1", 1, [waiting, account("a", "claude", 2, [])]),
    };
    try {
      const overview = render(
        <Overview
          state={state}
          filter="all"
          onFilter={vi.fn()}
          search=""
          onSearch={vi.fn()}
          searchOpen={false}
          onSearchOpen={vi.fn()}
          onFit={vi.fn()}
          onAddAccount={vi.fn()}
          onIndicatorStyle={vi.fn()}
          onResume={vi.fn()}
          onOpenAccount={() => undefined}
          onReconnect={() => undefined}
        />,
      );
      expect(screen.getByRole("button", { name: `Details for ${label}` })).toBeDefined();
      expect(screen.getByText(/Manual refreshes.*are deferred/).textContent).toContain(
        `Manual refreshes for ${label} are deferred.`,
      );
      if (mode === "stable_aliases") {
        expect(overview.container.textContent).not.toContain(waiting.nickname);
      }
      overview.unmount();
      window.history.replaceState(null, "", "#/settings/accounts");
      render(<Settings state={state} actions={settingsActions()} />);
      const notice = screen.getByText(/Manual refreshes.*are deferred/);
      expect(notice.textContent).toContain(`Manual refreshes for ${label} are deferred.`);
      if (mode === "stable_aliases") {
        expect(notice.textContent).not.toContain(waiting.nickname);
      }
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("effective refresh deadline rendering", () => {
  it.each([
    {
      name: "short",
      nextAttempt: "2026-10-01T12:05:00.000Z",
      initial: "5m",
      afterMinute: "4m",
      deadline: 300_000,
    },
    {
      name: "long",
      nextAttempt: "2026-10-01T13:00:00.000Z",
      initial: "1h 0m",
      afterMinute: "59m",
      deadline: 3_600_000,
    },
  ])("formats the snapshot deadline after $name Retry-After", (reading) => {
    const now = Date.parse("2026-10-01T12:00:00.000Z");
    const waiting = {
      ...account("a1", "codex", 1, []),
      fetch_state: "backoff" as const,
      last_attempt_at: "2026-10-01T12:00:00.000Z",
      next_attempt_at: reading.nextAttempt,
    };
    const notice = render(
      <RefreshNotice accounts={[waiting]} preferences={null} now={now} />,
    );
    expect(screen.getByRole("status").textContent).toContain(
      `Next eligible read in ${reading.initial}.`,
    );
    expect(screen.getByRole("status").textContent).not.toContain(reading.nextAttempt);
    notice.rerender(
      <RefreshNotice accounts={[waiting]} preferences={null} now={now + 60_000} />,
    );
    expect(screen.getByRole("status").textContent).toContain(
      `Next eligible read in ${reading.afterMinute}.`,
    );
    notice.rerender(
      <RefreshNotice
        accounts={[waiting]}
        preferences={null}
        now={now + reading.deadline}
      />,
    );
    expect(screen.queryByRole("status")).toBeNull();
  });
});
