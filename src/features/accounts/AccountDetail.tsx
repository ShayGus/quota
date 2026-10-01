/**
 * Account details.
 *
 * Every applicable window is shown at the same time, with its own number, exact
 * boundary, scope, and freshness. There are no period tabs, so basic comparison
 * never depends on opening a second surface (spec 3.2, AC-55).
 */
import { useState, type JSX } from "react";

import type { AccountSnapshot, QuotaWindow } from "../../generated/bindings";
import {
  arcFraction,
  formatRemaining,
  hasReading,
  severityOf,
} from "../../shared/format/allowance";
import {
  formatAge,
  formatBoundary,
  formatExactInstant,
  instantOf,
} from "../../shared/format/duration";
import { providerLabel } from "../../shared/format/provider";
import { Icon } from "../../shared/ui/Icon";
import { ProviderMark } from "../../shared/ui/ProviderMark";
import { Ring } from "../../shared/ui/Meter";
import { freshnessCaption, readingState } from "../overview/freshness";
import { statusOf } from "../overview/status";

/** The time zone used for exact boundary times. */
export const DISPLAY_TIME_ZONE = "Asia/Jerusalem";

/** The words for where a reading came from. */
const SOURCE_WORDS: Record<QuotaWindow["source"], string> = {
  documented_api: "Documented API",
  documented_cli_protocol: "Documented command line",
  observed_web_endpoint: "Observed web endpoint",
  local_capture: "Local capture",
  manual: "Entered by hand",
};

/**
 * The card title of each standard window. A window outside the standard three
 * is named by its own scope, because no column name would describe it.
 */
const WINDOW_TITLES: Record<QuotaWindow["category"], string | null> = {
  session: "5-hour",
  weekly: "Weekly",
  monthly: "Monthly",
  daily: null,
  custom: null,
};

/** The words for what an allowance represents in the plan. */
const ROLE_WORDS: Record<QuotaWindow["metric_role"], string> = {
  included_allowance: "Included allowance",
  extra_spend_cap: "Extra-spend cap",
  credit_balance: "Credit balance",
};

/** One window card: its reading, its boundary, and what it covers. */
function WindowCard({
  window,
  account,
  now,
  selected,
  onSelect,
}: {
  readonly window: QuotaWindow;
  readonly account: AccountSnapshot;
  readonly now: number;
  readonly selected: boolean;
  readonly onSelect: (windowId: QuotaWindow["id"]) => void;
}): JSX.Element {
  // The same freshness the overview uses, so a window that has gone stale or
  // whose boundary has passed cannot look healthy here (spec 6, AC-15).
  const state = readingState(account, window, now);
  const severity = state === "current" ? severityOf(window.measurement) : "stale";
  const value = formatRemaining(window.measurement);
  const hasValue = hasReading(window.measurement);
  return (
    <button
      type="button"
      className={`limit-card${selected ? " limit-card--selected" : ""}`}
      aria-pressed={selected}
      aria-label={`${window.scope.label || "Allowance"}: ${value} remaining. ${formatBoundary(window.boundary, now)}`}
      onClick={() => {
        onSelect(window.id);
      }}
    >
      <h3>{WINDOW_TITLES[window.category] ?? (window.scope.label || "Allowance")}</h3>
      <Ring
        fraction={arcFraction(window.measurement)}
        severity={severity}
        label={value}
        caption={hasValue ? freshnessCaption(state) : "no reading"}
      />
      <p className="limit-card__boundary">
        {window.boundary === null
          ? "No reported reset"
          : formatBoundary(window.boundary, now)}
      </p>
      <p className="limit-card__instant">
        {window.boundary === null
          ? "No reported reset"
          : formatExactInstant(window.boundary.at, DISPLAY_TIME_ZONE)}
      </p>
      <p className="limit-card__role">{ROLE_WORDS[window.metric_role]}</p>
      <p className="limit-card__scope">{window.scope.label || "Allowance"}</p>
    </button>
  );
}

/** The account details surface. */
export function AccountDetail({
  account,
  now,
  onBack,
  label,
}: {
  readonly account: AccountSnapshot;
  readonly now: number;
  readonly onBack: () => void;
  /** The name to show: the account's own, or its alias under the privacy setting. */
  readonly label: string;
}): JSX.Element {
  const [selectedWindow, setSelectedWindow] = useState<QuotaWindow["id"] | null>(
    account.windows[0]?.id ?? null,
  );
  const status = statusOf(account);
  const lastSuccess = instantOf(account.last_success_at);
  const lastAttempt = instantOf(account.last_attempt_at);
  const selected = account.windows.find((window) => window.id === selectedWindow) ?? null;
  const controllingWindowId =
    account.order.kind === "ranked" ? account.order.value.controlling_window_id : null;
  const controlling =
    controllingWindowId === null
      ? undefined
      : account.windows.find((window) => window.id === controllingWindowId);
  const controllingValue =
    controlling === undefined
      ? "no current reading"
      : formatRemaining(controlling.measurement);
  return (
    <section className="detail" aria-label={`Account details for ${label}`}>
      <div className="detail__back">
        <button type="button" className="back-button" onClick={onBack}>
          <Icon name="arrow-left" size={13} />
          All accounts
        </button>
        <span className="eyebrow">Account details</span>
      </div>
      <div className="detail__identity">
        <div className="identity">
          <ProviderMark providerId={account.provider_id} />
          <div>
            <h2>{label}</h2>
            <p>
              {providerLabel(account.provider_id)}
              {account.identity?.workspace_label != null
                ? ` · ${account.identity.workspace_label}`
                : ""}
              {account.identity?.plan_label != null
                ? ` · ${account.identity.plan_label}`
                : ""}
            </p>
          </div>
        </div>
        <span className={`badge badge--${status.tone}`}>
          <Icon name={status.icon} size={11} />
          {status.text}
        </span>
      </div>
      {account.windows.length === 0 ? (
        <p className="note">This account reports no allowance windows yet.</p>
      ) : (
        <div className="detail__grid">
          {account.windows.map((window) => (
            <WindowCard
              key={window.id}
              window={window}
              account={account}
              now={now}
              selected={window.id === selectedWindow}
              onSelect={setSelectedWindow}
            />
          ))}
        </div>
      )}
      <dl className="detail__list">
        <div>
          <dt>Account</dt>
          <dd>{account.nickname}</dd>
        </div>
        <div>
          <dt>Provider</dt>
          <dd>{providerLabel(account.provider_id)}</dd>
        </div>
        <div>
          <dt>Workspace</dt>
          <dd>{account.identity?.workspace_label ?? "Not reported"}</dd>
        </div>
        <div>
          <dt>Plan</dt>
          <dd>{account.identity?.plan_label ?? "Not reported"}</dd>
        </div>
        <div>
          <dt>Identity source</dt>
          <dd>
            {account.identity === null
              ? "Not verified"
              : SOURCE_WORDS[account.identity.source]}
          </dd>
        </div>
        <div>
          <dt>Connection</dt>
          <dd>
            {account.connection_state} · generation{" "}
            {String(account.connection_generation)}
          </dd>
        </div>
        <div>
          <dt>Last accepted reading</dt>
          <dd>{lastSuccess === null ? "None yet" : formatAge(lastSuccess, now)}</dd>
        </div>
        <div>
          <dt>Last attempt</dt>
          <dd>{lastAttempt === null ? "None yet" : formatAge(lastAttempt, now)}</dd>
        </div>
        <div>
          <dt>Missing windows</dt>
          <dd>
            {account.expected_but_missing_window_ids.length === 0
              ? "None"
              : account.expected_but_missing_window_ids.join(", ")}
          </dd>
        </div>
        {selected === null ? null : (
          <div>
            <dt>{selected.scope.label || "Allowance"} boundary</dt>
            <dd>
              {selected.boundary === null
                ? "Not reported"
                : formatExactInstant(selected.boundary.at, DISPLAY_TIME_ZONE)}
            </dd>
          </div>
        )}
      </dl>
      {account.order.kind === "unranked" ? (
        <p className="note">
          This account is in “Needs checking” because its rank reason is “
          {account.order.value.reason}”. Its last known percentage is never used as a
          current sorting value.
        </p>
      ) : (
        <p className="note">
          Ranked by {account.order.value.scope_label}: {controllingValue}. Rule version{" "}
          {String(account.order.value.rule_version)}.
        </p>
      )}
    </section>
  );
}
