/**
 * Privacy settings.
 *
 * Aliases replace account labels on screen; they are stable and distinguishable,
 * never one repeated word (AC-72). Local history and export identity are
 * separate choices.
 */
import { useId, useState, type JSX } from "react";

import type { Preferences, PrivacyAliasMode } from "../../../generated/bindings";
import { Icon } from "../../../shared/ui/Icon";
import { SettingRow, Select, Switch } from "../Primitives";
import type { SettingsActions } from "../Settings";
import { withAliasMode, withExportIdentities, withRetainHistory } from "../preferences";

/** The alias modes. */
const ALIAS_MODES: readonly (readonly [PrivacyAliasMode, string])[] = [
  ["off", "Show account labels"],
  ["stable_aliases", "Stable aliases"],
];

/**
 * The retention periods, and which of them this build can actually apply.
 *
 * The host has no retention sweep, so only Disabled and keeping indefinitely are
 * real choices; the two durations the mockup offers are shown but refused.
 */
const RETENTION: readonly (readonly [
  "indefinite" | "0" | "7" | "30",
  string,
  boolean?,
])[] = [
  ["indefinite", "Keep indefinitely"],
  ["0", "Disabled"],
  ["7", "7 days", true],
  ["30", "30 days", true],
];
/**
 * The diagnostic export row.
 *
 * The destination is chosen by the person and validated by the host. This window
 * never invents a path, and the export contains no tokens, no cookies, and no
 * provider payloads.
 */
export function DiagnosticExportRow({
  actions,
}: {
  readonly actions: SettingsActions;
}): JSX.Element {
  const id = useId();
  const [destination, setDestination] = useState("quota-diagnostics.json");
  const usable = destination.trim().length > 0;
  return (
    <div className="setting-row">
      <div>
        <label className="setting-row__label" htmlFor={id}>
          Diagnostic export
        </label>
        <p>
          Writes a sanitized report through the backend. Off by default it excludes
          account identities. Tokens, cookies, and provider payloads are never included.
        </p>
      </div>
      <span className="setting-row__actions">
        <input
          id={id}
          type="text"
          value={destination}
          maxLength={120}
          onChange={(event) => {
            setDestination(event.currentTarget.value);
          }}
        />
        <button
          type="button"
          className="button button--small"
          disabled={!usable}
          onClick={() => {
            actions.exportDiagnostics(destination.trim());
          }}
        >
          <Icon name="download" size={13} />
          Export
        </button>
      </span>
    </div>
  );
}

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
        label="Local history retention"
        description="Keeps normalized history for the retention period. Current readings and backoff are always kept."
        control={
          <Select
            label="Local history retention"
            value={preferences.privacy.retain_history ? "indefinite" : "0"}
            options={RETENTION}
            onChange={(value) => {
              actions.savePreferences(withRetainHistory(preferences, value !== "0"));
            }}
          />
        }
      />
      <p className="note">
        This build has no retention sweep, so history is kept until you clear it. The 7
        days and 30 days periods cannot be selected yet.
      </p>
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
    </>
  );
}
