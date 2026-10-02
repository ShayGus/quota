import { useState, type JSX } from "react";

import type { NotificationPolicy, Preferences } from "../../../generated/bindings";
import type { SettingsActions } from "../Settings";
import { SettingRow, Switch } from "../Primitives";

/** The three allowance alerts, each with the remaining percentage it names. */
const THRESHOLD_ALERTS: readonly (readonly ["low" | "critical" | "exhausted", string])[] =
  [
    ["low", "20%"],
    ["critical", "10%"],
    ["exhausted", "0%"],
  ];

/** What the alerts read before the host has ever published them. */
const ALL_ALERTS: Record<"low" | "critical" | "exhausted", boolean> = {
  low: true,
  critical: true,
  exhausted: true,
};

export function NotificationsPanel({
  preferences,
  actions,
}: {
  readonly preferences: Preferences;
  readonly actions: SettingsActions;
}): JSX.Element {
  const [preview, setPreview] = useState(false);
  const policy = preferences.notifications;
  const save = (next: NotificationPolicy): void => {
    actions.savePreferences({ ...preferences, notifications: next });
  };
  const alerts = policy.thresholds.alerts ?? ALL_ALERTS;
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
            onChange={(enabled) => {
              save({ ...policy, enabled });
            }}
          />
        }
      />
      <p className="setting-group-label">Notify when remaining quota reaches</p>
      <div className="thresholds" aria-label="Notification thresholds">
        {THRESHOLD_ALERTS.map(([field, label]) => (
          <label key={field}>
            <input
              type="checkbox"
              disabled={!policy.enabled}
              checked={alerts[field]}
              onChange={(event) => {
                save({
                  ...policy,
                  thresholds: {
                    ...policy.thresholds,
                    alerts: { ...alerts, [field]: event.currentTarget.checked },
                  },
                });
              }}
            />
            {label}
          </label>
        ))}
      </div>
      <SettingRow
        label="Recovery alerts"
        description="Notify only after a fresh reading confirms recovery."
        control={
          <Switch
            label="Recovery alerts"
            checked={policy.recovery_enabled}
            onChange={(recovery_enabled) => {
              save({ ...policy, recovery_enabled });
            }}
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
            onChange={(enabled) => {
              save({
                ...policy,
                quiet_hours: enabled
                  ? { kind: "daily_utc", window: { from_minute: 1320, to_minute: 420 } }
                  : { kind: "never" },
              });
            }}
          />
        }
      />
      {quiet.kind === "daily_utc" ? (
        <div className="quiet-times">
          <input
            type="time"
            aria-label="Quiet hours start (UTC)"
            value={formatTime(quiet.window.from_minute)}
            onChange={(event) => {
              setTime("from_minute", event.currentTarget.value);
            }}
          />
          <span>to</span>
          <input
            type="time"
            aria-label="Quiet hours end (UTC)"
            value={formatTime(quiet.window.to_minute)}
            onChange={(event) => {
              setTime("to_minute", event.currentTarget.value);
            }}
          />
        </div>
      ) : null}
      <div className="wizard-action">
        <button
          type="button"
          className="button"
          onClick={() => {
            setPreview(!preview);
          }}
        >
          Preview notification
        </button>
      </div>
      {preview ? (
        <div className="note" role="status">
          <strong>Quota · allowance low</strong>
          <p>A current allowance has 20% remaining.</p>
          <button
            type="button"
            className="text-button"
            onClick={() => {
              setPreview(false);
            }}
          >
            Dismiss preview
          </button>
        </div>
      ) : null}
    </>
  );
}
