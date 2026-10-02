/**
 * Window and monitoring settings.
 *
 * Floating is a normal window mode, and always-on-top is an independent
 * preference. Changing topmost changes nothing else: not geometry, not the
 * chosen mode, not monitoring, not account order (spec 4.4, AC-57).
 */
import type { JSX } from "react";

import type {
  MonitoringState,
  OverviewWindowState,
  Preferences,
} from "../../../generated/bindings";
import { SettingRow, Select, Switch } from "../Primitives";
import type { SettingsActions } from "../Settings";
import { backgroundRefreshSeconds, withBackgroundRefresh } from "../polling";
import { withDensity } from "../preferences";

/** The window settings panel. */
export function WindowPanel({
  preferences,
  nativeWindow,
  monitoring,
  actions,
}: {
  readonly preferences: Preferences;
  readonly monitoring: MonitoringState | null;
  readonly nativeWindow: OverviewWindowState | null;
  readonly actions: SettingsActions;
}): JSX.Element {
  const confirmed = nativeWindow?.kind === "confirmed" ? nativeWindow.value : null;
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
      <h3 className="settings__title">Window &amp; monitoring</h3>
      <p className="settings__intro">
        Floating stays open when another application gets focus. Tray mode dismisses on an
        outside click.
      </p>
      <SettingRow
        label="Window mode"
        description="Floating is a normal window mode. It is not tied to always-on-top."
        control={
          <Select
            label="Window mode"
            value={preferences.overview_mode}
            options={[
              ["floating", "Floating window"],
              ["tray", "Tray popover"],
            ]}
            onChange={(mode) => {
              actions.setOverviewMode(mode);
            }}
          />
        }
      />
      <SettingRow
        label="Always on top"
        description="Optional and off by default. This does not enable or disable dragging."
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
      <SettingRow
        label="Window width"
        description="Use the wider view to compare accounts. A narrow-width command is unavailable; Try narrow view resets the floating window so you can resize its borders."
        control={
          <span className="setting-row__actions">
            <button
              type="button"
              className="button button--small"
              onClick={actions.resetPosition}
            >
              Try narrow view
            </button>
            <button
              type="button"
              className="button button--small"
              onClick={actions.fitToAccounts}
            >
              Use wide view
            </button>
          </span>
        }
      />
      <SettingRow
        label="Window position"
        description="Keyboard movement is unavailable in the host. Move with keys currently resets the window to a visible floating position; use the title bar to move it."
        control={
          <button
            type="button"
            className="button button--small"
            onClick={actions.resetPosition}
          >
            Move with keys
          </button>
        }
      />
      <SettingRow
        label="Launch at login"
        description="This saves startup behavior: on restores the chosen mode, off starts quietly in the tray. The host cannot register or unregister login startup yet."
        control={
          <Switch
            label="Launch at login"
            checked={preferences.launch_behavior === "restore_last_mode"}
            onChange={(enabled) =>
              actions.savePreferences({
                ...preferences,
                launch_behavior: enabled ? "restore_last_mode" : "quiet_in_tray",
              })
            }
          />
        }
      />
      <SettingRow
        label="Pause monitoring"
        description="Keep last-known readings. A paused view is not a current ranking."
        control={
          monitoring === null ? (
            <button
              type="button"
              role="switch"
              className="switch"
              aria-label="Pause monitoring"
              aria-checked={false}
              disabled
            />
          ) : (
            <Switch
              label="Pause monitoring"
              checked={monitoring.kind === "paused"}
              onChange={actions.setMonitoring}
            />
          )
        }
      />
      <SettingRow
        label="Background refresh"
        description="Per-provider floors, shared-credential limits, and backoff take precedence. This changes the ordinary read schedule, keeping the provider's strategy."
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
            <option value="" disabled>
              {intervals.length === 0
                ? "No provider policy yet"
                : "Provider-specific intervals"}
            </option>
            <option value="60">Every minute</option>
            <option value="300">Every 5 minutes</option>
            <option value="900">Every 15 minutes</option>
          </select>
        }
      />
      <SettingRow
        label="Row density"
        description="Compact fits ten accounts without scrolling. Comfortable gives larger targets."
        control={
          <Select
            label="Row density"
            value={preferences.density}
            options={[
              ["compact", "Compact"],
              ["comfortable", "Comfortable"],
            ]}
            onChange={(density) => {
              actions.savePreferences(withDensity(preferences, density));
            }}
          />
        }
      />
      <SettingRow
        label="Window geometry"
        description="Fit widens the overview to the account layout. Reset returns it to a visible work area."
        control={
          <span className="setting-row__actions">
            <button
              type="button"
              className="button button--small"
              onClick={() => {
                actions.fitToAccounts();
              }}
            >
              Fit all accounts
            </button>
            <button
              type="button"
              className="button button--small"
              onClick={() => {
                actions.resetPosition();
              }}
            >
              Reset position
            </button>
          </span>
        }
      />
      <dl className="detail__list">
        <div>
          <dt>Confirmed mode</dt>
          <dd>
            {confirmed === null ? "Not confirmed by the system yet" : confirmed.mode}
          </dd>
        </div>
        <div>
          <dt>Confirmed always on top</dt>
          <dd>
            {confirmed === null ? "Not confirmed yet" : String(confirmed.always_on_top)}
          </dd>
        </div>
        <div>
          <dt>Confirmed visibility</dt>
          <dd>
            {confirmed === null
              ? "Not confirmed"
              : confirmed.visible
                ? "Shown"
                : "Hidden"}
          </dd>
        </div>
      </dl>
    </>
  );
}
