/**
 * The settings controls the approved design fixes in place.
 *
 * The three notification thresholds are separate choices and local history is a
 * retention selector, so these drive the real panels and read the preference
 * object each interaction saves (spec 13.2).
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { Preferences } from "../src/generated/bindings";
import { NotificationsPanel } from "../src/features/settings/panels/NotificationsPanel";
import { PrivacyPanel } from "../src/features/settings/panels/PrivacyPanel";
import type { SettingsActions } from "../src/features/settings/Settings";
import { preferences } from "./fixtures";

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
      setOverviewMode: vi.fn(),
      fitToAccounts: vi.fn(),
      resetPosition: vi.fn(),
      setAccountEnabled: vi.fn(),
      renameAccount: vi.fn(),
      disconnectAccount: vi.fn(),
      openUsagePage: vi.fn(),
      beginConnection: vi.fn(() => Promise.resolve({ id: "attempt-1" })),
      cancelConnection: vi.fn(() => Promise.resolve(undefined)),
      reconnectAccount: vi.fn(() => Promise.resolve(undefined)),
      clearHistory: vi.fn(),
      exportDiagnostics: vi.fn(),
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

describe("local history retention", () => {
  it("offers the approved selector and refuses the periods this build cannot apply", () => {
    const { actions, saved } = settingsActions();
    render(<PrivacyPanel preferences={preferences()} actions={actions} />);

    const select = screen.getByLabelText("Local history retention");
    const options = [...(select as HTMLSelectElement).options];
    expect(options.map((option) => option.textContent)).toEqual([
      "Keep indefinitely",
      "Disabled",
      "7 days",
      "30 days",
    ]);
    expect(options.find((option) => option.textContent === "7 days")?.disabled).toBe(
      true,
    );
    expect(options.find((option) => option.textContent === "30 days")?.disabled).toBe(
      true,
    );

    fireEvent.change(select, { target: { value: "0" } });

    expect(saved.at(-1)?.privacy.retain_history).toBe(false);
  });
});
