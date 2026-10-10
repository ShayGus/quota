/**
 * The prepaid-balance part of quota detail: what the gauge is measured from,
 * how fast the balance is being spent, what was spent in each period, and the
 * top-ups Quota has seen. Every amount comes from the host's ledger; a value it
 * did not report is left out rather than shown as zero.
 */
import type { JSX } from "react";

import type { BalanceSummary, KeySpend, QuotaWindow } from "../../generated/bindings";
import { remainingPercent } from "../../shared/format/allowance";
import {
  baselineLine,
  creditKindLabel,
  expiryLine,
  formatAmount,
  formatDay,
  runwayLine,
} from "../../shared/format/balance";

/** The eyebrow, headline, and lines beside a prepaid balance's large ring. */
export function balanceSummaryCopy(
  window: QuotaWindow,
  balance: BalanceSummary,
): {
  readonly eyebrow: string;
  readonly headline: string;
  readonly lines: readonly string[];
} {
  const percent = remainingPercent(window.measurement);
  const credits = balance.baseline_kind === "credits";
  const lines = [
    baselineLine(balance),
    credits ? expiryLine(balance) : null,
    runwayLine(balance),
  ].filter((line): line is string => line !== null);
  return {
    eyebrow: credits ? "LEFT OF ACTIVE CREDITS" : "LEFT OF LAST TOP-UP",
    headline: percent === null ? "Not reported" : `${percentWords(percent)} left`,
    lines: lines.length > 0 ? lines : ["Measured from the last top-up Quota saw"],
  };
}

/** A share, rounded down near full so a nearly full balance never reads 100%. */
function percentWords(percent: number): string {
  if (percent >= 100) {
    return "100%";
  }
  if (percent > 0 && percent < 1) {
    return "<1%";
  }
  return `${String(Math.floor(Math.max(0, percent)))}%`;
}

/** The measurement row of a prepaid balance: spent and left of its baseline. */
export function balanceMeasurement(balance: BalanceSummary): string {
  const money = (minor: number | null): string | null =>
    formatAmount(minor, balance.scale, balance.currency);
  const left = money(balance.balance_minor);
  const of = money(balance.baseline_minor);
  if (left === null) {
    return "Not available";
  }
  const spent =
    balance.baseline_minor !== null && balance.balance_minor !== null
      ? money(Math.max(0, balance.baseline_minor - balance.balance_minor))
      : null;
  if (spent === null || of === null) {
    return `${left} left`;
  }
  return `${spent} spent / ${left} left of ${of}`;
}

/**
 * What the account's API key spent, in its own section: a key reports it
 * whether or not it may read the account's balance.
 */
export function KeySpendDetail({ spend }: { readonly spend: KeySpend }): JSX.Element {
  const money = (minor: number | null): string =>
    formatAmount(minor, spend.scale, spend.currency) ?? "Not reported";
  const { periods } = spend;
  return (
    <>
      <h3 className="section-title">This key</h3>
      <dl className="detail-list" aria-label="This key">
        <div>
          <dt>This key spent</dt>
          <dd>
            {money(periods.today_minor)} today
            <br />
            <span className="muted">
              {money(periods.week_minor)} this week · {money(periods.month_minor)} this
              month
            </span>
          </dd>
        </div>
      </dl>
    </>
  );
}

/** The balance's spending and top-ups, under the selected balance's facts. */
export function BalanceDetail({
  balance,
}: {
  readonly balance: BalanceSummary;
}): JSX.Element {
  const money = (minor: number | null | undefined): string =>
    formatAmount(minor ?? null, balance.scale, balance.currency) ?? "Not reported";
  const credits = balance.baseline_kind === "credits";
  const cycle = balance.cycle_spend;
  return (
    <>
      <h3 className="section-title">Balance</h3>
      <dl className="detail-list" aria-label="Balance">
        {cycle === null ? null : (
          <div>
            <dt>Spent in {cycle.label}</dt>
            <dd>{money(cycle.spent_minor)}</dd>
          </div>
        )}
        <div>
          <dt>Spending pace</dt>
          <dd>{runwayLine(balance) ?? "Shown after a day of readings with spending"}</dd>
        </div>
        <div>
          <dt>{credits ? "Active credits" : "Since the account opened"}</dt>
          <dd>
            {money(balance.loaded_minor)} {credits ? "granted" : "loaded"}
            <br />
            <span className="muted">
              {money(balance.spent_minor)} {credits ? "used" : "spent"}
            </span>
          </dd>
        </div>
      </dl>
      {balance.credits.length === 0 ? null : (
        <>
          <h3 className="section-title">Credits</h3>
          <ul className="top-up-list" aria-label="Credits">
            {balance.credits.map((credit) => (
              <li key={`${credit.expires_at}-${String(credit.amount_minor)}`}>
                <span>
                  {creditKindLabel(credit.kind)} · expires{" "}
                  {formatDay(credit.expires_at) ?? "Not reported"}
                </span>
                <strong>
                  {money(credit.remaining_minor)} of {money(credit.amount_minor)}
                </strong>
              </li>
            ))}
          </ul>
        </>
      )}
      <h3 className="section-title">Top-ups</h3>
      {balance.top_ups.length === 0 ? (
        <div className="note">
          No top-up seen yet. The balance is measured from{" "}
          {balance.baseline_kind === "adjusted"
            ? "the last time it was adjusted"
            : credits
              ? "the credits that are still active"
              : "when you added the account"}
          .
        </div>
      ) : (
        <ul className="top-up-list" aria-label="Top-ups">
          {balance.top_ups.map((topUp) => (
            <li key={`${topUp.detected_at}-${String(topUp.amount_minor)}`}>
              <span>{formatDay(topUp.detected_at) ?? "Not reported"}</span>
              <strong>{money(topUp.amount_minor)}</strong>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
