import { useState, type JSX } from "react";

import type {
  AccountId,
  AccountSnapshot,
  NotificationPolicy,
  Preferences,
  QuotaWindow,
} from "../../../generated/bindings";
import { formatRemaining, remainingPercent } from "../../../shared/format/allowance";
import { boundaryCountdown } from "../../../shared/format/duration";
import { providerLabel } from "../../../shared/format/provider";
import { Icon } from "../../../shared/ui/Icon";
import { windowLabel } from "../../overview/reading";
import type { SettingsActions } from "../Settings";
import { SettingRow, SettingsTitle, Switch } from "../Primitives";

/** The three allowance alerts, each with the remaining percentage it names. */
const THRESHOLD_ALERTS: readonly (readonly ["low" | "critical" | "exhausted", string])[] =
  [
    ["low", "20%"],
    ["critical", "10%"],
    ["exhausted", "0%"],
  ];

/** A notification preview, drawn from a real account's current reading. */
interface Preview {
  readonly title: string;
  readonly body: string;
  readonly accountId: AccountId | null;
}

/**
 * The preview for the account closest to exhaustion, in the words its alert
 * would use. With no reading to show, the preview says so instead of
 * inventing one.
 */
export function previewFor(accounts: readonly AccountSnapshot[], now: number): Preview {
  let best: { account: AccountSnapshot; window: QuotaWindow; percent: number } | null =
    null;
  for (const account of accounts) {
    if (!account.monitoring_enabled || account.order.kind !== "ranked") continue;
    const { remaining_percent: percent, controlling_window_id: windowId } =
      account.order.value;
    const window = account.windows.find((candidate) => candidate.id === windowId);
    if (percent === null || window === undefined) continue;
    if (best === null || percent < best.percent) best = { account, window, percent };
  }
  if (best === null) {
    return {
      title: "No allowance to preview yet",
      body: "Connect an account with a current reading to preview its alert.",
      accountId: null,
    };
  }
  const { window } = best;
  // A standard period reads in lower case inside a sentence; a provider's own
  // name for a custom allowance is kept as given.
  const periodName =
    window.category === "custom"
      ? windowLabel(window)
      : windowLabel(window).toLowerCase();
  const percent = remainingPercent(window.measurement) ?? best.percent;
  // A level is named only when the reading has reached it, so the preview
  // never calls a healthy allowance low.
  const level =
    percent <= 0
      ? " is exhausted"
      : percent <= 10
        ? " is critical"
        : percent <= 20
          ? " is low"
          : "";
  const reset =
    window.boundary === null
      ? "no reported reset"
      : `resets in ${boundaryCountdown(window.boundary, now)}`;
  return {
    title: `${providerLabel(best.account.provider_id)}’s ${periodName} allowance${level}`,
    body: `${formatRemaining(window.measurement)} remaining · ${reset}.`,
    accountId: best.account.account_id,
  };
}

/** What the alerts read before the host has ever published them. */
const ALL_ALERTS: Record<"low" | "critical" | "exhausted", boolean> = {
  low: true,
  critical: true,
  exhausted: true,
};

export function NotificationsPanel({
  preferences,
  accounts,
  now,
  actions,
}: {
  readonly preferences: Preferences;
  readonly accounts: readonly AccountSnapshot[];
  readonly now: number;
  readonly actions: SettingsActions;
}): JSX.Element {
  const [preview, setPreview] = useState(false);
  const shownPreview = previewFor(accounts, now);
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
      <SettingsTitle
        title="Notifications"
        intro="Optional alerts for low allowances and restored access."
      />
      <SettingRow
        label="Enable notifications"
        description="Desktop alerts from verified quota readings."
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
      <div className="setting-group-label">Notify when remaining quota reaches</div>
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
        description="Suppress noncritical notifications during this daily interval (UTC)."
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
        <div className="setting-row">
          <span className="small muted">Quiet interval (UTC)</span>
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
        </div>
      ) : null}
      <div style={{ marginTop: "20px" }}>
        <button
          type="button"
          className="button"
          onClick={() => {
            setPreview(true);
          }}
        >
          <Icon name="bell" />
          Preview notification
        </button>
      </div>
      <div className="note">
        A preview stays inside this window and never triggers an operating system
        notification. Alerts are deduplicated within each quota window.
      </div>
      {preview ? (
        <div className="notification" role="status">
          <div className="notification-top">
            <span>Quota · notification preview</span>
            <button
              type="button"
              className="icon-btn"
              aria-label="Dismiss notification preview"
              onClick={() => {
                setPreview(false);
              }}
            >
              <Icon name="close" />
            </button>
          </div>
          <h3>{shownPreview.title}</h3>
          <p>{shownPreview.body}</p>
          <button
            type="button"
            className="text-btn"
            onClick={() => {
              setPreview(false);
              if (shownPreview.accountId === null) {
                actions.showOverview();
              } else {
                actions.showAccountDetail(shownPreview.accountId);
              }
            }}
          >
            View allowance
            <Icon name="arrow-right" />
          </button>
        </div>
      ) : null}
    </>
  );
}
