/**
 * Window and monitoring settings.
 *
 * Floating is a normal window mode, and always-on-top is an independent
 * preference. Changing topmost changes nothing else: not geometry, not the
 * chosen mode, not monitoring, not account order (spec 4.4, AC-57).
 */
import type { JSX } from "react";

import type { OverviewWindowState, Preferences } from "../../../generated/bindings";
import { SettingRow, Select, Switch } from "../Primitives";
import type { SettingsActions } from "../Settings";
import { withDensity } from "../preferences";

/** The window settings panel. */
export function WindowPanel({
  preferences,
  nativeWindow,
  actions,
}: {
  readonly preferences: Preferences;
  readonly nativeWindow: OverviewWindowState | null;
  readonly actions: SettingsActions;
}): JSX.Element {
  const confirmed = nativeWindow?.kind === "confirmed" ? nativeWindow.value : null;
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
