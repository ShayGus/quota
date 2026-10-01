/**
 * One account row.
 *
 * The row shows the identity, every applicable standard window at once, and one
 * status column naming the affected window (spec 3.1). Row identity is the
 * account ID, never its position, so a reorder never moves a click target onto
 * a different account (spec 4.3).
 */
import type { JSX } from "react";

import type {
  AccountSnapshot,
  IndicatorStyle,
  ProviderId,
} from "../../generated/bindings";
import { formatRemaining } from "../../shared/format/allowance";
import { formatAge, instantOf } from "../../shared/format/duration";
import { Icon } from "../../shared/ui/Icon";
import { COLUMNS, QuotaCell, columnLabel, windowsFor, type Column } from "./QuotaCell";
import { statusOf } from "./status";

/** The provider marks from the wireframe. Text only; no provider artwork is bundled. */
const PROVIDER_MARKS: Record<ProviderId, string> = {
  codex: ">_",
  claude: "\u2733",
  open_code_go: "GO",
  fixture: "FX",
};

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
      aria-label={`${label}, ${account.provider_id}, ${age}`}
    >
      <button
        type="button"
        className="identity identity--button"
        aria-label={`Details for ${label}`}
        onClick={() => {
          onOpen(account.account_id);
        }}
      >
        <span
          className={`provider-mark provider-mark--${account.provider_id}`}
          aria-hidden="true"
        >
          {PROVIDER_MARKS[account.provider_id]}
        </span>
        <span className="identity__copy">
          <span className="identity__name">{label}</span>
          <span className="identity__meta">
            {account.provider_id}
            {workspace != null ? ` · ${workspace}` : ""}
            {` · #${String(account.connection_ordinal)}`}
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
        <small>least remaining first</small>
      </span>
      {COLUMNS.map((column) => (
        <span key={column}>{columnLabel(column)}</span>
      ))}
      <span>State</span>
    </div>
  );
}
