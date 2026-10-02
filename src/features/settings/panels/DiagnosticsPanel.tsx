/**
 * Diagnostics.
 *
 * A sanitized view of what this window knows, and the diagnostic export. The
 * export destination is validated by the host, and the export never contains
 * tokens, cookies, or provider payloads.
 */
import type { JSX } from "react";

import type { MonitoringState } from "../../../generated/bindings";
import { displayName } from "../../../shared/format/alias";
import { formatAge, instantOf } from "../../../shared/format/duration";
import { providerLabel } from "../../../shared/format/provider";
import type { RendererState } from "../../../shared/state/types";
import { Icon } from "../../../shared/ui/Icon";
import { SettingRow, SettingsTitle, Switch } from "../Primitives";
import type { SettingsActions } from "../Settings";
import { withExportIdentities } from "../preferences";

/** The application version this panel reports. */
const APP_VERSION = "0.1.0";

/** The file the export is written to. The host resolves and validates it. */
const EXPORT_DESTINATION = "quota-diagnostics.json";

function monitoringWords(monitoring: MonitoringState | null): string {
  if (monitoring === null) {
    return "Not reported yet";
  }
  return monitoring.kind === "paused" ? "Paused" : "Running";
}

/** The diagnostics panel. */
export function DiagnosticsPanel({
  state,
  now,
  actions,
}: {
  readonly state: RendererState;
  readonly now: number;
  readonly actions: SettingsActions;
}): JSX.Element {
  const accounts = state.snapshot?.accounts ?? [];
  const preferences = state.preferences;
  const log = accounts.map((account) => {
    const checked = instantOf(account.last_success_at);
    return [
      providerLabel(account.provider_id),
      displayName(preferences, accounts, account),
      account.connection_state.replaceAll("_", " "),
      account.fetch_state,
      checked === null ? "not checked yet" : `checked ${formatAge(checked, now)}`,
    ].join(" · ");
  });
  return (
    <>
      <SettingsTitle
        title="Diagnostics"
        intro="Inspect Quota's local state. No provider payloads or credentials are collected."
      />
      <dl className="detail-list">
        <div>
          <dt>Application</dt>
          <dd>Quota · {APP_VERSION}</dd>
        </div>
        <div>
          <dt>Backend link</dt>
          <dd>{state.link}</dd>
        </div>
        <div>
          <dt>Enabled accounts</dt>
          <dd>
            {accounts.filter((account) => account.monitoring_enabled).length} /{" "}
            {accounts.length}
          </dd>
        </div>
        <div>
          <dt>Monitoring</dt>
          <dd>{monitoringWords(state.monitoring)}</dd>
        </div>
      </dl>
      <h4 className="section-title">Account status</h4>
      <div className="diagnostic-log">
        {log.length === 0 ? "No connected accounts." : log.join("\n")}
      </div>
      <button
        type="button"
        className="button"
        onClick={() => {
          actions.exportDiagnostics(EXPORT_DESTINATION);
        }}
      >
        <Icon name="download" />
        Export diagnostics
      </button>
      {preferences === null ? null : (
        <SettingRow
          label="Include identities in diagnostics"
          description="Off by default. Account labels are added to the export only when this is on."
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
      )}
      <div className="note">
        The export is sanitized: it never contains tokens, cookies, or provider payloads.
      </div>
    </>
  );
}
