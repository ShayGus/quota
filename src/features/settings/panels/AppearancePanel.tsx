/**
 * Appearance settings.
 *
 * Theme, indicator style, and reduced motion are committed preferences. The
 * theme also has a live preview effect, because the window must paint the chosen
 * scheme immediately.
 */
import type { JSX } from "react";

import type { Preferences, Theme } from "../../../generated/bindings";
import { SettingRow, Select, Switch } from "../Primitives";
import type { SettingsActions } from "../Settings";
import {
  withDensity,
  withIndicatorStyle,
  withReduceMotion,
  withTheme,
} from "../preferences";

/** The three colour-scheme options. */
const THEMES: readonly (readonly [Theme, string])[] = [
  ["system", "Follow system"],
  ["light", "Light"],
  ["dark", "Dark"],
];

/** The appearance settings panel. */
export function AppearancePanel({
  preferences,
  actions,
}: {
  readonly preferences: Preferences;
  readonly actions: SettingsActions;
}): JSX.Element {
  return (
    <>
      <h3 className="settings__title">Appearance</h3>
      <p className="settings__intro">
        The same account rows, drawn with rings or with compact bars. Both styles show the
        session, weekly, and monthly limits together.
      </p>
      <div className="theme-options">
        {THEMES.map(([theme, label]) => (
          <button
            key={theme}
            type="button"
            className="theme-option"
            aria-pressed={preferences.theme === theme}
            onClick={() => {
              actions.savePreferences(withTheme(preferences, theme));
            }}
          >
            <div className={`theme-thumbnail theme-thumbnail--${theme}`}>
              <span>
                <i />
                <i />
                <i />
              </span>
            </div>
            <b>{label}</b>
          </button>
        ))}
      </div>
      <SettingRow
        label="Allowance indicators"
        description="Rings show a draining arc. Bars show the same values horizontally."
        control={
          <Select
            label="Allowance indicators"
            value={preferences.indicator_style}
            options={[
              ["ring", "Rings"],
              ["bar", "Bars"],
            ]}
            onChange={(style) => {
              actions.savePreferences(withIndicatorStyle(preferences, style));
            }}
          />
        }
      />
      <SettingRow
        label="Row density"
        description="Compact fits ten accounts without scrolling at the reference window size."
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
        label="Reduce motion"
        description="Suppresses animation. A value change is still shown, without movement."
        control={
          <Switch
            checked={preferences.reduce_motion}
            label="Reduce motion"
            onChange={(next) => {
              actions.savePreferences(withReduceMotion(preferences, next));
            }}
          />
        }
      />
    </>
  );
}
