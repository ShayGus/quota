import { useState, type JSX } from "react";

import type { NotificationPolicy, Preferences } from "../../../generated/bindings";
import type { SettingsActions } from "../Settings";
import { SettingRow, Switch } from "../Primitives";

export function NotificationsPanel({
  preferences,
  actions,
}: {
  readonly preferences: Preferences;
  readonly actions: SettingsActions;
}): JSX.Element {
  const [preview, setPreview] = useState(false);
  const policy = preferences.notifications;
  const save = (next: NotificationPolicy): void =>
    actions.savePreferences({ ...preferences, notifications: next });
  const quiet = policy.quiet_hours;
  const formatTime = (minutes: number): string =>
    `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
  const setTime = (field: "from_minute" | "to_minute", time: string): void => {
    if (quiet.kind !== "daily_utc" || time === "") return;
    const [hour, minute] = time.split(":").map(Number);
    save({
      ...policy,
      quiet_hours: {
        kind: "daily_utc",
        window: { ...quiet.window, [field]: (hour ?? 0) * 60 + (minute ?? 0) },
      },
    });
  };
  return (
    <>
      <h3 className="settings__title">Notifications</h3>
      <p className="settings__intro">
        Optional alerts for low allowances and restored access.
      </p>
      <SettingRow
        label="Enable notifications"
        description="Receive alerts from verified quota readings."
        control={
          <Switch
            label="Enable notifications"
            checked={policy.enabled}
            onChange={(enabled) => save({ ...policy, enabled })}
          />
        }
      />
      <div className="thresholds" aria-label="Notification thresholds">
        {(
          [
            ["low_percent", "20%", 20],
            ["critical_percent", "10%", 10],
            ["critical_percent", "0%", 0],
          ] as const
        ).map(([field, label, value]) => (
          <label key={label}>
            <input
              type="checkbox"
              disabled={!policy.enabled}
              checked={policy.thresholds[field] === value}
              onChange={(event) =>
                save({
                  ...policy,
                  thresholds: {
                    ...policy.thresholds,
                    [field]: event.currentTarget.checked ? value : null,
                  },
                })
              }
            />
            {label}
          </label>
        ))}
      </div>
      <p className="settings__intro">
        Choose 10% or 0% for the critical alert; the host supports one critical threshold.
      </p>
      <SettingRow
        label="Recovery alerts"
        description="Notify only after a fresh reading confirms recovery."
        control={
          <Switch
            label="Recovery alerts"
            checked={policy.recovery_enabled}
            onChange={(recovery_enabled) => save({ ...policy, recovery_enabled })}
          />
        }
      />
      <SettingRow
        label="Quiet hours"
        description="Suppress alerts during this daily UTC interval."
        control={
          <Switch
            label="Quiet hours"
            checked={quiet.kind === "daily_utc"}
            onChange={(enabled) =>
              save({
                ...policy,
                quiet_hours: enabled
                  ? { kind: "daily_utc", window: { from_minute: 1320, to_minute: 420 } }
                  : { kind: "never" },
              })
            }
          />
        }
      />
      {quiet.kind === "daily_utc" ? (
        <div className="quiet-times">
          <input
            type="time"
            aria-label="Quiet hours start (UTC)"
            value={formatTime(quiet.window.from_minute)}
            onChange={(event) => setTime("from_minute", event.currentTarget.value)}
          />
          <span>to</span>
          <input
            type="time"
            aria-label="Quiet hours end (UTC)"
            value={formatTime(quiet.window.to_minute)}
            onChange={(event) => setTime("to_minute", event.currentTarget.value)}
          />
        </div>
      ) : null}
      <div className="wizard-action">
        <button type="button" className="button" onClick={() => setPreview(!preview)}>
          Preview notification
        </button>
      </div>
      {preview ? (
        <div className="note" role="status">
          <strong>Quota · allowance low</strong>
          <p>A current allowance has 20% remaining.</p>
          <button type="button" className="text-button" onClick={() => setPreview(false)}>
            Dismiss preview
          </button>
        </div>
      ) : null}
    </>
  );
}
