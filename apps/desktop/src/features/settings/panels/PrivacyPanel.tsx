/**
 * Privacy settings.
 *
 * Aliases replace account labels on screen; they are stable and distinguishable,
 * never one repeated word (AC-72). Local history and export identity are
 * separate choices.
 */
import type { JSX } from "react";

import type { Preferences, PrivacyAliasMode } from "../../../generated/bindings";
import { Icon } from "../../../shared/ui/Icon";
import { SettingRow, Select, Switch } from "../Primitives";
import type { SettingsActions } from "../Settings";
import {
  withAliasMode,
  withExportIdentities,
  withRetainHistory,
} from "../preferences";

/** The alias modes. */
const ALIAS_MODES: readonly (readonly [PrivacyAliasMode, string])[] = [
  ["off", "Show account labels"],
  ["stable_aliases", "Stable aliases"],
];

/** The privacy settings panel. */
export function PrivacyPanel({
  preferences,
  actions,
}: {
  readonly preferences: Preferences;
  readonly actions: SettingsActions;
}): JSX.Element {
  return (
    <>
      <h3 className="settings__title">Privacy</h3>
      <p className="settings__intro">
        A local view of your allowances. Monitoring never reads conversation content or
        session titles.
      </p>
      <div className="privacy-card">
        <Icon name="shield" size={18} />
        <div>
          <strong>No credentials reach this window</strong>
          <p>
            Tokens and cookies stay in the operating system credential store. The renderer
            receives sanitized snapshots only.
          </p>
        </div>
      </div>
      <SettingRow
        label="Hide account labels"
        description="Replaces nicknames and workspace labels with stable, distinguishable aliases."
        control={
          <Select
            label="Hide account labels"
            value={preferences.privacy.alias_mode}
            options={ALIAS_MODES}
            onChange={(mode) => {
              actions.savePreferences(withAliasMode(preferences, mode));
            }}
          />
        }
      />
      <SettingRow
        label="Retain local history"
        description="Keeps normalized history for the retention period. Current readings and backoff are always kept."
        control={
          <Switch
            checked={preferences.privacy.retain_history}
            label="Retain local history"
            onChange={(next) => {
              actions.savePreferences(withRetainHistory(preferences, next));
            }}
          />
        }
      />
      <SettingRow
        label="Include identities in diagnostics"
        description="Off by default. The export never contains tokens, cookies, or provider payloads."
        control={
          <Switch
            checked={preferences.privacy.export_identities}
            label="Include identities in diagnostics"
            onChange={(next) => {
              actions.savePreferences(withExportIdentities(preferences, next));
            }}
          />
        }
      />
      <SettingRow
        label="Diagnostic export"
        description="Writes a sanitized report through the backend. The destination is chosen by the system, never by this window."
        control={
          <button
            type="button"
            className="button button--small"
            onClick={() => {
              actions.exportDiagnostics();
            }}
          >
            <Icon name="download" size={13} />
            Export diagnostics
          </button>
        }
      />
      <SettingRow
        label="Clear local history"
        description="Drops retained history for every account. Active account bindings and retry state are kept."
        control={
          <button
            type="button"
            className="button button--small button--danger"
            onClick={() => {
              actions.clearHistory(null);
            }}
          >
            Clear history
          </button>
        }
      />
    </>
  );
}
