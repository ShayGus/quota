/**
 * Appearance settings.
 *
 * Theme, overview layout, and reduced motion are committed preferences. The
 * theme also has a live preview effect, because the window must paint the chosen
 * scheme immediately.
 */
import type { JSX } from "react";

import type {
  AccountSnapshot,
  AccountSort,
  IndicatorStyle,
  Preferences,
  Theme,
} from "../../../generated/bindings";
import { SettingRow, SettingsTitle, Switch } from "../Primitives";
import type { SettingsActions } from "../Settings";
import {
  withAccountSort,
  withIndicatorStyle,
  withReduceMotion,
  withTheme,
} from "../preferences";

/** The three colour-scheme options, in the wireframe's order. */
const THEMES: readonly (readonly [Theme, string])[] = [
  ["light", "Light"],
  ["dark", "Dark"],
  ["system", "System"],
];

/** The two overview layouts. */
const LAYOUTS: readonly (readonly [IndicatorStyle, string])[] = [
  ["ring", "Donuts"],
  ["bar", "Compact"],
];

/** The account orders, the automatic one first. */
const SORTS: readonly (readonly [AccountSort, string])[] = [
  ["least_remaining", "Least left"],
  ["manual", "My order"],
  ["provider", "Provider"],
];

/** The appearance settings panel. */
export function AppearancePanel({
  preferences,
  accounts,
  actions,
}: {
  readonly preferences: Preferences;
  /** The accounts, so "My order" can start from the order on screen. */
  readonly accounts: readonly AccountSnapshot[];
  readonly actions: SettingsActions;
}): JSX.Element {
  return (
    <>
      <SettingsTitle
        title="Appearance"
        intro="The same information, in a layout that suits your desktop."
      />
      <div className="theme-options">
        {THEMES.map(([theme, label]) => (
          <button
            key={theme}
            type="button"
            className={`theme-option${preferences.theme === theme ? " selected" : ""}`}
            aria-pressed={preferences.theme === theme}
            onClick={() => {
              actions.savePreferences(withTheme(preferences, theme));
            }}
          >
            <div className={`theme-thumbnail ${theme}`}>
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
        label="Overview layout"
        description="Comfortable rings or a denser list of bars."
        control={
          <div className="tabs" role="group" aria-label="Overview layout">
            {LAYOUTS.map(([style, label]) => (
              <button
                key={style}
                type="button"
                className={preferences.indicator_style === style ? "selected" : ""}
                aria-pressed={preferences.indicator_style === style}
                onClick={() => {
                  actions.savePreferences(withIndicatorStyle(preferences, style));
                }}
              >
                {label}
              </button>
            ))}
          </div>
        }
      />
      <SettingRow
        label="Account order"
        description={
          preferences.account_sort === "manual"
            ? "Arrange accounts with the arrows in Accounts. Readings never move them."
            : preferences.account_sort === "provider"
              ? "Grouped by provider, in the order you added them."
              : "What needs checking first, then the least allowance left."
        }
        control={
          <div className="tabs" role="group" aria-label="Account order">
            {SORTS.map(([sort, label]) => (
              <button
                key={sort}
                type="button"
                className={preferences.account_sort === sort ? "selected" : ""}
                aria-pressed={preferences.account_sort === sort}
                onClick={() => {
                  actions.savePreferences(withAccountSort(preferences, sort, accounts));
                }}
              >
                {label}
              </button>
            ))}
          </div>
        }
      />
      <SettingRow
        label="Reduce motion"
        description="Disable small transitions. A value change is still shown, without movement."
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
      <div className="note">
        Each arc represents remaining allowance. Status colors are consistent across
        providers; percentages are not combined.
      </div>
    </>
  );
}
