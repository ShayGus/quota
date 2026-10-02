/**
 * General settings: when Quota runs and how the popover behaves.
 *
 * Floating or docking lives on the popover header. Always on top is an
 * independent preference that applies in both modes: changing it changes
 * nothing else, not geometry, not the chosen mode, not monitoring, not account
 * order (spec 4.4, AC-57).
 */
import { useEffect, useState, type JSX } from "react";

import { launch } from "../../../shared/ipc/report";

import type { MonitoringState, Preferences } from "../../../generated/bindings";
import { SettingRow, SettingsTitle, Switch } from "../Primitives";
import type { SettingsActions } from "../Settings";
import { backgroundRefreshSeconds, withBackgroundRefresh } from "../polling";

/** The general settings panel. */
export function WindowPanel({
  preferences,
  monitoring,
  actions,
}: {
  readonly preferences: Preferences;
  readonly monitoring: MonitoringState | null;
  readonly actions: SettingsActions;
}): JSX.Element {
  // The login item lives in the operating system, not in the preferences, so
  // the switch shows what the system reports and nothing before it answers.
  const [launchAtLogin, setLaunchAtLogin] = useState<boolean | null>(null);
  useEffect(() => {
    let current = true;
    launch(
      actions.launchAtLogin().then((confirmed) => {
        if (current) setLaunchAtLogin(confirmed);
      }),
    );
    return () => {
      current = false;
    };
  }, [actions]);
  const intervals = preferences.polling.map(backgroundRefreshSeconds);
  const uniformInterval = intervals[0];
  const refresh =
    uniformInterval !== undefined &&
    intervals.every((interval) => interval === uniformInterval) &&
    [60, 300, 900].includes(uniformInterval)
      ? String(uniformInterval)
      : "";
  return (
    <>
      <SettingsTitle
        title="General"
        intro="Control when Quota runs and how the tray window behaves."
      />
      <SettingRow
        label="Launch at login"
        description="Start quietly in the tray when you sign in."
        control={
          <Switch
            checked={launchAtLogin === true}
            label="Launch at login"
            disabled={launchAtLogin === null}
            onChange={(next) => {
              launch(
                actions.setLaunchAtLogin(next).then((confirmed) => {
                  if (confirmed !== null) setLaunchAtLogin(confirmed);
                }),
              );
            }}
          />
        }
      />
      <SettingRow
        label="Pause monitoring"
        description="Keep last-known readings; do not request new ones."
        control={
          <Switch
            label="Pause monitoring"
            checked={monitoring?.kind === "paused"}
            disabled={monitoring === null}
            onChange={actions.setMonitoring}
          />
        }
      />
      <SettingRow
        label="Background refresh"
        description="Provider-specific limits and backoff take precedence."
        control={
          <select
            aria-label="Background refresh"
            value={refresh}
            disabled={preferences.polling.length === 0}
            onChange={(event) => {
              const seconds = Number(event.currentTarget.value);
              for (const policy of preferences.polling)
                actions.savePollingPreferences(withBackgroundRefresh(policy, seconds));
            }}
          >
            {refresh === "" ? (
              <option value="" disabled>
                {intervals.length === 0
                  ? "No provider policy yet"
                  : "Provider-specific intervals"}
              </option>
            ) : null}
            <option value="60">Every minute</option>
            <option value="300">Every 5 minutes</option>
            <option value="900">Every 15 minutes</option>
          </select>
        }
      />
      <SettingRow
        label="Always on top"
        description="Keeps Quota above other windows, docked to the tray or floating."
        control={
          <Switch
            checked={preferences.always_on_top}
            label="Always on top"
            onChange={(next) => {
              actions.setAlwaysOnTop(next);
            }}
          />
        }
      />
      <div className="note">
        Preferences are saved on this device and apply immediately. Launch at login is the
        only system setting Quota changes, and only when you turn it on.
      </div>
    </>
  );
}
