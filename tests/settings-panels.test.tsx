/**
 * The settings controls the approved design fixes in place.
 *
 * The three notification thresholds are separate choices, local history is a
 * retention selector, and account actions confirm in a dialog, so these drive
 * the real panels and read what each interaction saves (spec 13.2).
 */
import { fireEvent, render, screen, within } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import type { Preferences } from "../src/generated/bindings";
import { AppearancePanel } from "../src/features/settings/panels/AppearancePanel";
import { DiagnosticsPanel } from "../src/features/settings/panels/DiagnosticsPanel";
import {
  NotificationsPanel,
  previewFor,
} from "../src/features/settings/panels/NotificationsPanel";
import { PrivacyPanel } from "../src/features/settings/panels/PrivacyPanel";
import { AccountsPanel } from "../src/features/settings/panels/AccountsPanel";
import type { SettingsActions } from "../src/features/settings/Settings";
import { initialRendererState } from "../src/shared/state/types";
import {
  account,
  NOW,
  percent,
  preferences,
  snapshot,
  window as quotaWindow,
} from "./fixtures";

/** Records every preference object the panel saves. */
function settingsActions(): { actions: SettingsActions; saved: Preferences[] } {
  const saved: Preferences[] = [];
  return {
    saved,
    actions: {
      savePreferences: (next) => {
        saved.push(next);
      },
      setMonitoring: vi.fn(),
      savePollingPreferences: vi.fn(),
      setAlwaysOnTop: vi.fn(),
      setAppView: vi.fn(),
      setOverviewMode: vi.fn(),
      setAccountEnabled: vi.fn(),
      renameAccount: vi.fn(),
      setKeyLimitShown: vi.fn(),
      setGroupSpendShown: vi.fn(),
      setGroupKeyShown: vi.fn(),
      createAccountGroup: vi.fn(),
      setAccountGroup: vi.fn(),
      renameAccountGroup: vi.fn(),
      disconnectAccount: vi.fn(),
      openUsagePage: vi.fn(),
      beginConnection: vi.fn(() => Promise.resolve({ id: "attempt-1" })),
      cancelConnection: vi.fn(() => Promise.resolve(undefined)),
      confirmConnection: vi.fn(() => Promise.resolve(true)),
      reconnectAccount: vi.fn(() => Promise.resolve(undefined)),
      clearHistory: vi.fn(),
      exportDiagnostics: vi.fn(() =>
        Promise.resolve("/data/diagnostics/quota-diagnostics-settings.json"),
      ),
      openBugReportIssue: vi.fn(() => Promise.resolve()),
      copyBugReportPrompt: vi.fn(() => Promise.resolve(true)),
      showOverview: vi.fn(),
      showAccountDetail: vi.fn(),
      launchAtLogin: vi.fn(() => Promise.resolve(false)),
      setLaunchAtLogin: vi.fn((launch: boolean) => Promise.resolve(launch)),
    },
  };
}

/** The checkbox named by its visible percentage. */
function alert(label: string): HTMLInputElement {
  const found = screen.getByLabelText(label);
  if (!(found instanceof HTMLInputElement)) {
    throw new Error(`${label} is not a checkbox`);
  }
  return found;
}

describe("notification thresholds", () => {
  it("switches the three percentages independently", () => {
    const { actions, saved } = settingsActions();
    const base = preferences();
    render(
      <NotificationsPanel
        preferences={{
          ...base,
          notifications: { ...base.notifications, enabled: true },
        }}
        accounts={[]}
        now={NOW}
        actions={actions}
      />,
    );

    expect(alert("20%").checked).toBe(true);
    expect(alert("10%").checked).toBe(true);
    expect(alert("0%").checked).toBe(true);

    fireEvent.click(alert("0%"));

    const last = saved.at(-1);
    expect(last).toBeDefined();
    // The shared critical percentage is untouched, so 10% stays switched on.
    expect(last?.notifications.thresholds.critical_percent).toBe(10);
    expect(last?.notifications.thresholds.alerts?.exhausted).toBe(false);
    expect(last?.notifications.thresholds.alerts?.critical).toBe(true);
  });

  it("never writes a null percentage", () => {
    const { actions, saved } = settingsActions();
    const base = preferences();
    render(
      <NotificationsPanel
        preferences={{
          ...base,
          notifications: { ...base.notifications, enabled: true },
        }}
        accounts={[]}
        now={NOW}
        actions={actions}
      />,
    );

    fireEvent.click(alert("20%"));

    const thresholds = saved.at(-1)?.notifications.thresholds;
    expect(thresholds?.low_percent).toBe(20);
    expect(thresholds?.critical_percent).toBe(10);
    expect(thresholds?.alerts?.low).toBe(false);
  });
});

describe("the notification preview", () => {
  it("previews the real account closest to exhaustion and never notifies the system", () => {
    const { actions, saved } = settingsActions();
    const low = account("a1", "claude", 1, [quotaWindow("w1", "session", percent(18))], {
      rank: 18,
    });
    const healthy = account(
      "a2",
      "codex",
      2,
      [quotaWindow("w2", "weekly", percent(64))],
      {
        rank: 64,
      },
    );
    render(
      <NotificationsPanel
        preferences={preferences()}
        accounts={[healthy, low]}
        now={NOW}
        actions={actions}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Preview notification" }));
    const status = screen.getByRole("status").textContent;
    expect(status).toContain("Claude’s 5-hour allowance is low");
    expect(status).toContain("18% remaining · resets in 2h 0m.");
    expect(status).not.toMatch(/illustrative|sample|demo/i);
    fireEvent.click(screen.getByRole("button", { name: /View allowance/ }));
    expect(actions.showAccountDetail).toHaveBeenCalledWith("a1");
    expect(screen.queryByRole("status")).toBeNull();
    expect(saved).toHaveLength(0);
  });

  it("never calls a healthy allowance low", () => {
    const healthy = account(
      "a2",
      "codex",
      2,
      [quotaWindow("w2", "weekly", percent(64))],
      {
        rank: 64,
      },
    );
    expect(previewFor([healthy], NOW).title).toBe("Codex’s weekly allowance");
  });

  it("says there is nothing to preview instead of inventing a reading", () => {
    const { actions } = settingsActions();
    render(
      <NotificationsPanel
        preferences={preferences()}
        accounts={[]}
        now={NOW}
        actions={actions}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Preview notification" }));
    expect(screen.getByRole("status").textContent).toContain(
      "No allowance to preview yet",
    );
    fireEvent.click(screen.getByRole("button", { name: /View allowance/ }));
    expect(actions.showOverview).toHaveBeenCalledTimes(1);
  });
});

describe("privacy", () => {
  it("says plainly what Quota sends and keeps, without claims that are no longer true", () => {
    const { actions } = settingsActions();
    render(<PrivacyPanel preferences={preferences()} accounts={[]} actions={actions} />);
    for (const heading of [
      "No Quota server, no tracking",
      "It only talks to your AI providers",
      "Your sign-ins stay protected",
      "Your work stays yours",
    ]) {
      expect(screen.getByText(heading)).toBeTruthy();
    }
    // Keys are pasted into settings, so the panel must not say none reach it.
    expect(screen.queryByText(/No credentials reach this window/)).toBeNull();
  });

  it("keeps or stops keeping reading history from a switch", () => {
    const { actions, saved } = settingsActions();
    render(
      <PrivacyPanel
        preferences={preferences({
          privacy: { ...preferences().privacy, retain_history: true },
        })}
        accounts={[]}
        actions={actions}
      />,
    );
    const keep = screen.getByRole("switch", { name: "Keep reading history" });
    expect(keep.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(keep);
    expect(saved.at(-1)?.privacy.retain_history).toBe(false);
  });

  it("hides account names with stable aliases from a switch", () => {
    const { actions, saved } = settingsActions();
    render(<PrivacyPanel preferences={preferences()} accounts={[]} actions={actions} />);
    const hide = screen.getByRole("switch", { name: "Hide account names" });
    expect(hide.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(hide);
    expect(saved.at(-1)?.privacy.alias_mode).toBe("stable_aliases");
  });

  it("clears every account's history only after confirmation", () => {
    const { actions } = settingsActions();
    render(
      <PrivacyPanel
        preferences={preferences()}
        accounts={[account("a1", "codex", 1, []), account("a2", "claude", 2, [])]}
        actions={actions}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Clear…" }));
    expect(actions.clearHistory).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog", { name: "Clear reading history?" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(actions.clearHistory).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Clear…" }));
    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Clear" }),
    );
    expect(vi.mocked(actions.clearHistory).mock.calls).toEqual([["a1"], ["a2"]]);
  });
});

describe("appearance", () => {
  it("saves the theme and the overview layout", () => {
    const { actions, saved } = settingsActions();
    render(
      <AppearancePanel preferences={preferences()} accounts={[]} actions={actions} />,
    );
    expect(
      screen.getByRole("button", { name: "Dark" }).getAttribute("aria-pressed"),
    ).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "System" }));
    expect(saved.at(-1)?.theme).toBe("system");
    fireEvent.click(screen.getByRole("button", { name: "Compact" }));
    expect(saved.at(-1)?.indicator_style).toBe("bar");
  });
});

describe("diagnostics", () => {
  it("exports a sanitized report and states each account's status", async () => {
    const { actions } = settingsActions();
    render(
      <DiagnosticsPanel
        state={{
          ...initialRendererState,
          link: "live",
          preferences: preferences(),
          snapshot: snapshot("instance-1", 1, [account("a1", "codex", 1, [])]),
        }}
        now={NOW}
        actions={actions}
      />,
    );
    expect(document.querySelector(".diagnostic-log")?.textContent).toBe(
      "Codex · a1 · connected · idle · checked 1m ago",
    );
    expect(
      screen.queryByRole("switch", { name: "Include identities in diagnostics" }),
    ).toBeNull();
    await act(() =>
      fireEvent.click(screen.getByRole("button", { name: "Export diagnostics" })),
    );
    expect(actions.exportDiagnostics).toHaveBeenCalledWith("settings");
    expect(screen.getByRole("status").textContent).toBe(
      "Saved to /data/diagnostics/quota-diagnostics-settings.json",
    );
  });
});

describe("account management identities", () => {
  it("distinguishes matching nicknames by principal and aliases both cards", () => {
    const { actions } = settingsActions();
    const accounts = [
      account("a1", "claude", 1, [], { nickname: "Work" }),
      account("a2", "claude", 2, [], { nickname: "Work" }),
    ];
    const base = preferences();
    const panel = (confirmed: Preferences) => (
      <AccountsPanel
        accounts={accounts}
        groups={[]}
        preferences={confirmed}
        actions={actions}
        onAddAccount={vi.fn()}
        onAddKey={vi.fn()}
      />
    );
    const { rerender } = render(panel(base));
    const cards = screen.getAllByRole("article", { name: "Manage Claude Work" });
    expect(cards).toHaveLength(2);
    for (const [index, card] of cards.entries()) {
      expect(card.textContent).toContain(`a${String(index + 1)}@example.test`);
      expect(card.textContent).toContain("Home");
      expect(card.textContent).toContain("Claude");
    }
    const secondCard = cards[1];
    if (secondCard === undefined) throw new Error("the second account card is missing");
    fireEvent.click(within(secondCard).getByRole("button", { name: "Details" }));
    expect(actions.showAccountDetail).toHaveBeenCalledWith("a2");

    rerender(
      panel({ ...base, privacy: { ...base.privacy, alias_mode: "stable_aliases" } }),
    );
    for (const ordinal of [1, 2]) {
      const card = screen.getByRole("article", {
        name: `Manage Claude Account ${String(ordinal)}`,
      });
      expect(card.textContent).toContain(`Account ${String(ordinal)}`);
      expect(card.textContent).toContain("Identity hidden");
      expect(card.textContent).not.toContain("@example.test");
      expect(card.textContent).not.toContain("Work");
      expect(card.textContent).not.toContain("Home");
    }
    rerender(panel(base));
    expect(screen.getByText(/a1@example.test/)).toBeTruthy();
    expect(screen.getByText(/a2@example.test/)).toBeTruthy();
  });

  it("keeps rename closed while account labels are hidden, and masks an open one", () => {
    const { actions } = settingsActions();
    const accounts = [account("a1", "claude", 1, [], { nickname: "Work" })];
    const aliased = preferences({
      privacy: {
        alias_mode: "stable_aliases",
        retain_history: false,
        export_identities: false,
      },
    });
    const { rerender } = render(
      <AccountsPanel
        accounts={accounts}
        groups={[]}
        preferences={aliased}
        actions={actions}
        onAddAccount={vi.fn()}
        onAddKey={vi.fn()}
      />,
    );
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Rename" }).disabled,
    ).toBe(true);

    rerender(
      <AccountsPanel
        accounts={accounts}
        groups={[]}
        preferences={preferences()}
        actions={actions}
        onAddAccount={vi.fn()}
        onAddKey={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    const rename = screen.getByRole("dialog", { name: "Rename account" });
    const field = within(rename).getByLabelText<HTMLInputElement>("Account nickname");
    expect(field.classList.contains("masked")).toBe(false);
    rerender(
      <AccountsPanel
        accounts={accounts}
        groups={[]}
        preferences={aliased}
        actions={actions}
        onAddAccount={vi.fn()}
        onAddKey={vi.fn()}
      />,
    );
    expect(field.classList.contains("masked")).toBe(true);
    expect(
      within(rename).getByText("Hidden while Hide account names is on."),
    ).toBeTruthy();
  });

  it("renames, reconnects, and disconnects one account through its dialog", () => {
    const { actions } = settingsActions();
    const accounts = [
      account("a1", "claude", 1, [], { nickname: "Work" }),
      account("a2", "claude", 2, [], { nickname: "Home" }),
    ];
    render(
      <AccountsPanel
        accounts={accounts}
        groups={[]}
        preferences={preferences()}
        actions={actions}
        onAddAccount={vi.fn()}
        onAddKey={vi.fn()}
      />,
    );
    const card = screen.getByRole("article", { name: "Manage Claude Home" });

    fireEvent.click(within(card).getByRole("button", { name: "Rename" }));
    const rename = screen.getByRole("dialog", { name: "Rename account" });
    fireEvent.change(within(rename).getByLabelText("Account nickname"), {
      target: { value: "  Side project " },
    });
    fireEvent.click(within(rename).getByRole("button", { name: "Save nickname" }));
    expect(actions.renameAccount).toHaveBeenCalledWith("a2", "Side project");
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.click(within(card).getByRole("button", { name: "Reconnect" }));
    const reconnect = screen.getByRole("dialog", { name: "Reconnect Claude" });
    expect(reconnect.textContent).toContain("a2@example.test");
    fireEvent.click(within(reconnect).getByRole("button", { name: "Reconnect" }));
    expect(actions.reconnectAccount).toHaveBeenCalledWith("a2");

    fireEvent.click(within(card).getByRole("button", { name: "Disconnect" }));
    const disconnect = screen.getByRole("dialog", { name: "Disconnect Claude?" });
    expect(disconnect.textContent).toContain("other Claude accounts stay connected");
    fireEvent.click(within(disconnect).getByRole("button", { name: "Cancel" }));
    expect(actions.disconnectAccount).not.toHaveBeenCalled();
    fireEvent.click(within(card).getByRole("button", { name: "Disconnect" }));
    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Disconnect" }),
    );
    expect(actions.disconnectAccount).toHaveBeenCalledWith("a2");
    expect(actions.setAccountEnabled).not.toHaveBeenCalled();
  });

  it("monitors and adds accounts, and states that order cannot be changed", () => {
    const { actions } = settingsActions();
    const onAddAccount = vi.fn();
    render(
      <AccountsPanel
        accounts={[account("a1", "codex", 1, [])]}
        groups={[]}
        preferences={preferences()}
        actions={actions}
        onAddAccount={onAddAccount}
        onAddKey={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("switch", { name: "Monitor Codex a1" }));
    expect(actions.setAccountEnabled).toHaveBeenCalledWith("a1", false);
    fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    expect(onAddAccount).toHaveBeenCalledTimes(1);
    for (const name of ["Move Codex a1 up", "Move Codex a1 down"])
      expect(screen.getByRole("button", { name })).toHaveProperty("disabled", true);
  });
});
