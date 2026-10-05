/**
 * Diagnostics.
 *
 * A sanitized view of what this window knows, and the diagnostic export. The
 * export destination is validated by the host, and the export never contains
 * tokens, cookies, or provider payloads.
 */
import { useState, type JSX } from "react";

import type { MonitoringState } from "../../../generated/bindings";
import { displayName } from "../../../shared/format/alias";
import { formatAge, instantOf } from "../../../shared/format/duration";
import { providerLabel } from "../../../shared/format/provider";
import type { RendererState } from "../../../shared/state/types";
import { Icon } from "../../../shared/ui/Icon";
import { PROMPT_COPIED } from "../../../shared/ui/ReportBug";
import { launch } from "../../../shared/ipc/report";
import { APP_VERSION } from "../../../shared/version";
import { SettingsTitle } from "../Primitives";
import type { SettingsActions } from "../Settings";

/**
 * The label the host puts in the export's file name. The host reduces it to a
 * safe stem and writes `quota-diagnostics-<label>.json` in its own diagnostics
 * folder; the renderer never chooses a path.
 */
const EXPORT_LABEL = "settings";

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
  const [saved, setSaved] = useState<string | null>(null);
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
          launch(
            actions.exportDiagnostics(EXPORT_LABEL).then((path) => {
              setSaved(path);
            }),
          );
        }}
      >
        <Icon name="download" />
        Export diagnostics
      </button>
      {saved === null ? null : (
        <p className="form-hint" role="status">
          Saved to {saved}
        </p>
      )}
      <div className="note">
        The export is sanitized: it holds the account count, providers, and polling
        settings, and never contains account identities, tokens, cookies, or provider
        payloads.
      </div>
      <ReportBugSection actions={actions} />
    </>
  );
}

/**
 * Report a bug: the same two choices as the header and the tray. Both carry
 * only the version, the system, the view and the connected providers.
 */
function ReportBugSection({
  actions,
}: {
  readonly actions: SettingsActions;
}): JSX.Element {
  const [copied, setCopied] = useState(false);
  return (
    <>
      <h4 className="section-title">Report a bug</h4>
      <p className="form-hint">
        Open a GitHub issue, or copy a prompt that asks your AI agent to file one for you.
        Either one includes Quota&apos;s version, your system, the view and the connected
        providers, never account names or keys.
      </p>
      <div className="button-row">
        <button
          type="button"
          className="button"
          onClick={() => {
            launch(actions.openBugReportIssue());
          }}
        >
          <Icon name="external" />
          Open an issue on GitHub
        </button>
        <button
          type="button"
          className="button"
          onClick={() => {
            launch(
              actions.copyBugReportPrompt().then((done) => {
                setCopied(done);
              }),
            );
          }}
        >
          <Icon name="terminal" />
          Copy a prompt for an AI agent
        </button>
      </div>
      {copied ? (
        <p className="form-hint" role="status">
          {PROMPT_COPIED}
        </p>
      ) : null}
    </>
  );
}
