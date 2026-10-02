/**
 * One quota cell.
 *
 * A cell either shows a reading or says, in words, why there is none. It never
 * draws a ring for an allowance the account does not offer (spec 3.2, AC-03),
 * and it never invents a number for a reading that has none (AC-07).
 */
import type { JSX } from "react";

import type {
  AccountSnapshot,
  IndicatorStyle,
  QuotaWindow,
} from "../../generated/bindings";
import {
  arcFraction,
  formatRemaining,
  hasReading,
  severityOf,
} from "../../shared/format/allowance";
import {
  boundaryCountdown,
  boundaryLead,
  formatBoundary,
} from "../../shared/format/duration";
import { Bar, Ring } from "../../shared/ui/Meter";
import { freshnessCaption, readingState, type ReadingState } from "./freshness";

/** The standard column order. Additional windows appear after it. */
export const COLUMNS: readonly ("session" | "weekly" | "monthly")[] = [
  "session",
  "weekly",
  "monthly",
];

/** One standard column. */
export type Column = (typeof COLUMNS)[number];

/** The label of one standard column. */
export function columnLabel(column: Column): string {
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
export function windowFor(account: AccountSnapshot, column: Column): QuotaWindow | null {
  return account.windows.find((window) => window.category === column) ?? null;
}

/**
 * Every window that belongs in one column.
 *
 * A category can carry more than one allowance: a Claude plan may report a
 * weekly allowance and a separate weekly allowance for one model. Taking only
 * the first hides the one that controls exhaustion, so all of them are returned
 * and the caller renders each.
 */
export function windowsFor(
  account: AccountSnapshot,
  column: Column,
): readonly QuotaWindow[] {
  return account.windows.filter(
    (window) => window.category === column && window.metric_role === "included_allowance",
  );
}

/** The words under one ring, from what the reading actually is. */
function captionOf(window: QuotaWindow, state: ReadingState): string {
  if (hasReading(window.measurement)) {
    return freshnessCaption(state);
  }
  switch (window.measurement.kind) {
    case "unlimited":
      return "unlimited";
    case "not_entitled":
      return "not included";
    case "unavailable":
    case "percentage":
    case "quantity":
    case "money":
      return "no reading";
  }
}

/** The accessible name of one quota cell. */
function cellLabel(
  account: AccountSnapshot,
  column: Column,
  window: QuotaWindow | null,
  now: number,
): string {
  const identity = `${account.nickname}, ${columnLabel(column)}`;
  if (window === null) {
    return `${identity}: not offered by this account`;
  }
  return `${identity}: ${formatRemaining(window.measurement)} remaining. ${formatBoundary(window.boundary, now)}`;
}

/** One window's reading cell. */
export function QuotaCell({
  account,
  column,
  window,
  style,
  now,
  onOpen,
}: {
  readonly account: AccountSnapshot;
  readonly column: Column;
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
        <span
          className="quota-cell__absent"
          aria-label={cellLabel(account, column, null, now)}
        >
          <b aria-hidden="true">—</b>
          <span>Not offered</span>
        </span>
      </div>
    );
  }
  // A reading that is not current is drawn muted and dashed, and its caption
  // says "last known", so it can never keep a healthy "Current" appearance
  // (spec 6, AC-15). Its number stays visible.
  const state = readingState(account, window, now);
  const severity = state === "current" ? severityOf(window.measurement) : "stale";
  const fraction = arcFraction(window.measurement);
  const value = formatRemaining(window.measurement);
  const reading = boundaryCountdown(window.boundary, now);
  const lead =
    window.boundary === null
      ? "Status"
      : `${state === "current" ? "" : "Not current · "}${boundaryLead(window.boundary)} in`;
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
              <small>
                {lead} · {reading}
              </small>
            </span>
            <Bar fraction={fraction} severity={severity} />
          </span>
        ) : (
          <>
            <Ring
              fraction={fraction}
              severity={severity}
              label={value}
              caption={captionOf(window, state)}
            />
            <span className="quota-cell__time">
              <small>{lead}</small>
              <strong>{reading}</strong>
            </span>
          </>
        )}
      </button>
    </div>
  );
}
