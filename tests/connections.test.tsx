import { fireEvent, render, screen } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import type { ConnectionAttemptAccepted } from "../src/generated/bindings";
import { AccountsPanel } from "../src/features/settings/panels/AccountsPanel";
import type { SettingsActions } from "../src/features/settings/Settings";
import { Overview } from "../src/features/overview/Overview";
import { initialRendererState } from "../src/shared/state/types";
import { account, snapshot } from "./fixtures";

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
    beginConnection: vi.fn(async () => null),
    clearConnectionAttempt: vi.fn(),
    reconnectAccount: vi.fn(async () => undefined),
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
      <AccountsPanel accounts={[]} attempts={[]} now={0} actions={actions} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));

    expect(
      (screen.getByRole("button", { name: "Connecting..." }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect((screen.getByLabelText("Add an account") as HTMLSelectElement).disabled).toBe(
      true,
    );
    expect(
      (screen.getByLabelText("Label for the new account") as HTMLInputElement).disabled,
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
      (screen.getByRole("button", { name: "Connecting..." }) as HTMLButtonElement).disabled,
    ).toBe(true);
    panel.rerender(
      <AccountsPanel
        accounts={[]}
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
      (screen.getByRole("button", { name: "Connect" }) as HTMLButtonElement).disabled,
    ).toBe(false);
    expect(
      screen.getByText("Connected. Quota is reading this account now."),
    ).toBeDefined();
  });

  it("releases the controls after acceptance is refused", async () => {
    const actions = settingsActions();
    render(<AccountsPanel accounts={[]} attempts={[]} now={0} actions={actions} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    });
    expect(
      (screen.getByRole("button", { name: "Connect" }) as HTMLButtonElement).disabled,
    ).toBe(false);
    expect(screen.getByText(/Quota could not start that connection/)).toBeDefined();
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
