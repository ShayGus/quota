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
  QuotaWindow,
} from "../../generated/bindings";
import {
  arcFraction,
  formatRemaining,
  remainingPercent,
  severityOf,
} from "../../shared/format/allowance";
import { formatAge, formatBoundary, instantOf } from "../../shared/format/duration";
import { Icon } from "../../shared/ui/Icon";
import { Bar, Ring } from "../../shared/ui/Meter";
import { statusOf } from "./status";

/** The provider marks from the wireframe. Text only; no provider artwork is bundled. */
const PROVIDER_MARKS: Record<ProviderId, string> = {
  codex: ">_",
  claude: "\u2733",
  clinepass: "C_",
  opencode_go: "GO",
  fixture: "FX",
};

/** The standard column order. Additional windows appear after it. */
const COLUMNS: readonly ("session" | "weekly" | "monthly")[] = [
  "session",
  "weekly",
  "monthly",
];

/** The label of one standard column. */
function columnLabel(column: "session" | "weekly" | "monthly"): string {
  switch (column) {
    case "session":
      return "5-hour";
    case "weekly":
      return "Weekly";
    case "monthly":
      return "Monthly";
  }
}

/** The account's window for one column, or `null` when it does not offer one. */
function windowFor(
  account: AccountSnapshot,
  column: "session" | "weekly" | "monthly",
): QuotaWindow | null {
  return account.windows.find((window) => window.category === column) ?? null;
}

/** The words under one ring, from what the reading actually is. */
function captionOf(window: QuotaWindow): string {
  const percent = remainingPercent(window.measurement);
  if (percent === null) {
    switch (window.measurement.kind) {
      case "unlimited":
        return "unlimited";
      case "not_entitled":
        return "not included";
      case "unavailable":
        return "no reading";
      default:
        return "no reading";
    }
  }
  if (window.valid_until !== null && instantOf(window.valid_until) !== null) {
    const deadline = instantOf(window.valid_until);
    if (deadline !== null && deadline <= Date.now()) {
      return "last known";
    }
  }
  return "left";
}

/** The accessible name of one quota cell. */
function cellLabel(
  account: AccountSnapshot,
  column: "session" | "weekly" | "monthly",
  window: QuotaWindow | null,
  now: number,
): string {
  const identity = `${account.nickname}, ${columnLabel(column)}`;
  if (window === null) {
    return `${identity}: not offered by this account`;
  }
  const reading = formatRemaining(window.measurement);
  return `${identity}: ${reading} remaining. ${formatBoundary(window.boundary, now)}`;
}

/** One window's reading cell. */
function QuotaCell({
  account,
  column,
  window,
  style,
  now,
  onOpen,
}: {
  readonly account: AccountSnapshot;
  readonly column: "session" | "weekly" | "monthly";
  readonly window: QuotaWindow | null;
  readonly style: IndicatorStyle;
  readonly now: number;
  readonly onOpen: (accountId: AccountSnapshot["account_id"]) => void;
}): JSX.Element {
  const label = columnLabel(column);
  if (window === null) {
    return (
      <div className="quota-cell">
        <span className="quota-cell__mobile-label">{label}</span>
        <span className="quota-cell__absent" aria-label={cellLabel(account, column, null, now)}>
          <b aria-hidden="true">—</b>
          <span>Not offered</span>
        </span>
      </div>
    );
  }
  const severity = severityOf(window.measurement);
  const fraction = arcFraction(window.measurement);
  const value = formatRemaining(window.measurement);
  return (
    <div className="quota-cell">
      <span className="quota-cell__mobile-label">{label}</span>
      <button
        type="button"
        className="quota-cell__button"
        aria-label={cellLabel(account, column, window, now)}
        onClick={() => {
          onOpen(account.account_id);
        }}
      >
        {style === "bar" ? (
          <span className="quota-cell__bar">
            <span className="quota-cell__bar-top">
              <strong>{value}</strong>
              <small>{formatBoundary(window.boundary, now)}</small>
            </span>
            <Bar fraction={fraction} severity={severity} />
          </span>
        ) : (
          <>
            <Ring
              fraction={fraction}
              severity={severity}
              label={value}
              caption={captionOf(window)}
            />
            <span className="quota-cell__time">
              <small>Resets</small>
              <strong>{formatBoundary(window.boundary, now)}</strong>
            </span>
          </>
        )}
      </button>
    </div>
  );
}

/** One account row. */
export function AccountRow({
  account,
  style,
  now,
  onOpen,
  onReconnect,
}: {
  readonly account: AccountSnapshot;
  readonly style: IndicatorStyle;
  readonly now: number;
  readonly onOpen: (accountId: AccountSnapshot["account_id"]) => void;
  readonly onReconnect: (accountId: AccountSnapshot["account_id"]) => void;
}): JSX.Element {
  const status = statusOf(account);
  const lastSuccess = instantOf(account.last_success_at);
  const age = lastSuccess === null ? "No accepted reading" : formatAge(lastSuccess, now);
  const extra = account.windows.filter(
    (window) =>
      window.category !== "session" &&
      window.category !== "weekly" &&
      window.category !== "monthly",
  );
  return (
    <article
      className={`account-row${account.order.kind === "unranked" ? " account-row--unknown" : ""}`}
      data-account-id={account.account_id}
      aria-label={`${account.nickname}, ${account.provider_id}, ${age}`}
    >
      <button
        type="button"
        className="identity identity--button"
        aria-label={`Details for ${account.nickname}`}
        onClick={() => {
          onOpen(account.account_id);
        }}
      >
        <span className={`provider-mark provider-mark--${account.provider_id}`} aria-hidden="true">
          {PROVIDER_MARKS[account.provider_id]}
        </span>
        <span className="identity__copy">
          <span className="identity__name">{account.nickname}</span>
          <span className="identity__meta">
            {account.provider_id}
            {account.identity?.workspace_label != null
              ? ` · ${account.identity.workspace_label}`
              : ""}
            {` · #${String(account.connection_ordinal)}`}
          </span>
        </span>
      </button>
      {COLUMNS.map((column) => (
        <QuotaCell
          key={column}
          account={account}
          column={column}
          window={windowFor(account, column)}
          style={style}
          now={now}
          onOpen={onOpen}
        />
      ))}
      <div className="row-state">
        <span className={`badge badge--${status.tone}`}>
          <Icon name={status.icon} size={11} />
          {status.text}
        </span>
        {account.connection_state === "reauthentication_required" ? (
          <button
            type="button"
            className="text-button"
            aria-label={`Reconnect ${account.nickname}`}
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
