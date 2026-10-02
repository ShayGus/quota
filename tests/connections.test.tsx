import { fireEvent, render, screen } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import type { ConnectionAttemptAccepted } from "../src/generated/bindings";
import { AccountsPanel } from "../src/features/settings/panels/AccountsPanel";
import { Settings, type SettingsActions } from "../src/features/settings/Settings";
import { Overview } from "../src/features/overview/Overview";
import { initialRendererState } from "../src/shared/state/types";
import { RefreshNotice } from "../src/shared/ui/RefreshNotice";
import { account, preferences, snapshot } from "./fixtures";

function settingsActions(): SettingsActions {
  return {
    savePreferences: vi.fn(),
    setAlwaysOnTop: vi.fn(),
    setOverviewMode: vi.fn(),
    fitToAccounts: vi.fn(),
    resetPosition: vi.fn(),
    setAccountEnabled: vi.fn(),
    renameAccount: vi.fn(),
    disconnectAccount: vi.fn(),
    openUsagePage: vi.fn(),
    beginConnection: vi.fn(() => Promise.resolve(null)),
    clearConnectionAttempt: vi.fn(),
    reconnectAccount: vi.fn(() => Promise.resolve()),
    clearHistory: vi.fn(),
    exportDiagnostics: vi.fn(),
  };
}

describe("pending connection acceptance", () => {
  it("blocks duplicate submissions and stays busy until terminal progress", async () => {
    const pending = Promise.withResolvers<ConnectionAttemptAccepted | null>();
    const beginConnection = vi.fn(() => pending.promise);
    const actions = { ...settingsActions(), beginConnection };
    const panel = render(
      <AccountsPanel
        accounts={[]}
        preferences={preferences()}
        attempts={[]}
        now={0}
        actions={actions}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));

    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Connecting..." }).disabled,
    ).toBe(true);
    expect(screen.getByLabelText<HTMLSelectElement>("Add an account").disabled).toBe(
      true,
    );
    expect(
      screen.getByLabelText<HTMLInputElement>("Label for the new account").disabled,
    ).toBe(true);
    const form = screen.getByRole("button", { name: "Connecting..." }).closest("form");
    if (form === null) {
      throw new Error("the connect form is missing");
    }
    fireEvent.submit(form);
    expect(beginConnection).toHaveBeenCalledTimes(1);

    await act(async () => {
      pending.resolve({ attempt_id: "attempt", attempt_ref: { id: "attempt" } });
      await pending.promise;
    });
    panel.rerender(
      <AccountsPanel
        accounts={[]}
        preferences={preferences()}
        attempts={[
          {
            attemptId: "attempt",
            revision: 1,
            progress: { kind: "awaiting_user" },
          },
        ]}
        now={0}
        actions={actions}
      />,
    );
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Connecting..." }).disabled,
    ).toBe(true);
    panel.rerender(
      <AccountsPanel
        accounts={[]}
        preferences={preferences()}
        attempts={[
          {
            attemptId: "attempt",
            revision: 2,
            progress: { kind: "verified", context: { state: "connected" } },
          },
        ]}
        now={0}
        actions={actions}
      />,
    );
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Connect" }).disabled,
    ).toBe(false);
    expect(
      screen.getByText("Connected. Quota is reading this account now."),
    ).toBeDefined();
  });

  it("releases the controls after acceptance is refused", async () => {
    const actions = settingsActions();
    render(
      <AccountsPanel
        accounts={[]}
        preferences={preferences()}
        attempts={[]}
        now={0}
        actions={actions}
      />,
    );
    await act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Connect" }));
      return Promise.resolve();
    });
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Connect" }).disabled,
    ).toBe(false);
    expect(screen.getByText(/Quota could not start that connection/)).toBeDefined();
  });
});

describe("settings connection session", () => {
  it("keeps pending acceptance and provider recovery across every tab", async () => {
    const pending = Promise.withResolvers<ConnectionAttemptAccepted | null>();
    const beginConnection = vi.fn(() => pending.promise);
    const actions = { ...settingsActions(), beginConnection };
    const state = { ...initialRendererState, preferences: preferences() };
    const settings = render(<Settings state={state} actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: "Accounts" }));
    fireEvent.change(screen.getByLabelText("Add an account"), {
      target: { value: "claude" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    for (const tab of ["Window", "Appearance", "Privacy"]) {
      fireEvent.click(screen.getByRole("button", { name: tab }));
      expect(screen.queryByRole("button", { name: "Connecting..." })).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: "Accounts" }));
      expect(
        screen.getByRole<HTMLButtonElement>("button", { name: "Connecting..." }).disabled,
      ).toBe(true);
    }
    expect(beginConnection).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Appearance" }));
    await act(async () => {
      pending.resolve({ attempt_id: "attempt", attempt_ref: { id: "attempt" } });
      await pending.promise;
    });
    fireEvent.click(screen.getByRole("button", { name: "Accounts" }));
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Connecting..." }).disabled,
    ).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Privacy" }));
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
    fireEvent.click(screen.getByRole("button", { name: "Accounts" }));
    expect(screen.getByText(/Claude Code is not signed in/)).toBeDefined();
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Connect" }).disabled,
    ).toBe(false);
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
        attempts={[]}
        now={now}
        actions={settingsActions()}
      />,
    );
    expect(screen.getByText(/Manual refreshes.*are deferred/).textContent).toContain(
      "5m",
    );
    panel.rerender(
      <AccountsPanel
        accounts={[waiting]}
        preferences={preferences()}
        attempts={[]}
        now={now + 300_000}
        actions={settingsActions()}
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
      render(<Settings state={state} actions={settingsActions()} />);
      fireEvent.click(screen.getByRole("button", { name: "Accounts" }));
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
