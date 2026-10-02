/**
 * One account row.
 *
 * The row shows the identity, every applicable standard window at once, and one
 * status column naming the affected window (spec 3.1). Row identity is the
 * account ID, never its position, so a reorder never moves a click target onto
 * a different account (spec 4.3).
 */
import type { JSX } from "react";

import type { AccountSnapshot, IndicatorStyle } from "../../generated/bindings";
import { formatRemaining } from "../../shared/format/allowance";
import { formatAge, instantOf } from "../../shared/format/duration";
import { providerLabel } from "../../shared/format/provider";
import { Icon } from "../../shared/ui/Icon";
import { ProviderMark } from "../../shared/ui/ProviderMark";
import { COLUMNS, QuotaCell, columnLabel, windowsFor, type Column } from "./QuotaCell";
import { statusOf } from "./status";

/** One account row. */
export function AccountRow({
  account,
  style,
  now,
  onOpen,
  onReconnect,
  label,
}: {
  readonly account: AccountSnapshot;
  readonly style: IndicatorStyle;
  readonly now: number;
  readonly onOpen: (accountId: AccountSnapshot["account_id"]) => void;
  readonly onReconnect: (accountId: AccountSnapshot["account_id"]) => void;
  /** The name to show: the account's own, or its alias under the privacy setting. */
  readonly label: string;
}): JSX.Element {
  const status = statusOf(account);
  const lastSuccess = instantOf(account.last_success_at);
  const age = lastSuccess === null ? "No accepted reading" : formatAge(lastSuccess, now);
  // Anything the standard columns do not show, including every extra-spend cap:
  // a cap is reported as itself rather than as the monthly allowance.
  const extra = account.windows.filter(
    (window) =>
      !COLUMNS.includes(window.category as Column) ||
      window.metric_role !== "included_allowance",
  );
  const workspace = account.identity?.workspace_label;
  return (
    <article
      className={`account-row${account.order.kind === "unranked" ? " account-row--unknown" : ""}`}
      data-account-id={account.account_id}
      aria-label={`${label}, ${providerLabel(account.provider_id)}, ${age}`}
    >
      <button
        type="button"
        className="identity identity--button"
        aria-label={`Details for ${label}`}
        onClick={() => {
          onOpen(account.account_id);
        }}
      >
        <ProviderMark providerId={account.provider_id} />
        <span className="identity__copy">
          <span className="identity__name">{label}</span>
          <span className="identity__meta">
            {providerLabel(account.provider_id)}
            {workspace != null ? ` · ${workspace}` : ""}
          </span>
        </span>
      </button>
      {COLUMNS.map((column) => {
        const windows = windowsFor(account, column);
        return (
          <div key={column} className="quota-column">
            {(windows.length === 0 ? [null] : windows).map((window) => (
              <QuotaCell
                key={window?.id ?? column}
                account={account}
                column={column}
                window={window}
                style={style}
                now={now}
                onOpen={onOpen}
              />
            ))}
          </div>
        );
      })}
      <div className="row-state">
        <span className={`badge badge--${status.tone}`}>
          <Icon name={status.icon} size={11} />
          {status.text}
        </span>
        {account.connection_state === "reauthentication_required" ? (
          <button
            type="button"
            className="text-button"
            aria-label={`Reconnect ${label}`}
            onClick={() => {
              onReconnect(account.account_id);
            }}
          >
            Reconnect
          </button>
        ) : (
          <span className="row-state__age">{age}</span>
        )}
        {extra.map((window) => (
          <span key={window.id} className="row-state__extra">
            {window.scope.label}: {formatRemaining(window.measurement)}
          </span>
        ))}
      </div>
    </article>
  );
}

/** The column header row, which also states the polarity of every number. */
export function AccountColumns(): JSX.Element {
  return (
    <div className="table__columns" role="presentation">
      <span>
        Account
        <small>Remaining →</small>
      </span>
      {COLUMNS.map((column) => (
        <span key={column}>{columnLabel(column)}</span>
      ))}
      <span>Status</span>
    </div>
  );
}
