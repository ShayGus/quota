/**
 * What the mini widget shows for each account.
 *
 * The widget draws the overview's readings, from the overview's own helpers,
 * in a smaller form: rings for the ring strip, rows for the mini cards. A period
 * keeps one colour everywhere, so teal is always the 5-hour window, blue the
 * week and violet the month. Money is shown as money, never as a share of it.
 * Nothing here invents a number for a reading that has none.
 */
import type {
  AccountId,
  AccountSnapshot,
  Preferences,
  ProviderId,
  QuotaWindow,
} from "../../generated/bindings";
import { displayName } from "../../shared/format/alias";
import { moneyLeft, remainingPercent } from "../../shared/format/allowance";
import { providerLabel } from "../../shared/format/provider";
import { placeAccounts } from "../../shared/state/order";
import { boundaryCountdown } from "../../shared/format/duration";
import {
  ACCOUNT_RESOURCE,
  viewFraction,
  viewKnown,
  viewValue,
  windowLabel,
  windowView,
  type WindowView,
} from "../overview/reading";
import { statusOf } from "../overview/status";

/** A period, which names a ring and gives it its colour. */
export type Period = QuotaWindow["category"];

/** The periods in ring order: the shortest is the outer ring. */
const PERIODS: readonly Period[] = ["session", "daily", "weekly", "monthly", "custom"];

/** The most rings a tile draws. */
const MAX_RINGS = 3;

/** At or below this share left, a number is drawn as low. */
const LOW_PERCENT = 20;

/** One ring: a period, and how much of it is left, or `null` for no arc. */
export interface WidgetRing {
  readonly period: Period;
  readonly fraction: number | null;
  /** The letter for the ring's period under the tile: H, D, W or M. */
  readonly letter: string;
  /** What is left in the ring, as it is written: `50%`, or `—`. */
  readonly value: string;
  readonly low: boolean;
}

/** One limit, as a mini card row or a line of the breakdown. */
export interface WidgetRow {
  /** The short label in front of the row: `5h`, `Wk`, `Fable`, `Extra`. */
  readonly tag: string;
  /** The full name: `5-hour`, `Fable weekly`, `Monthly · On-demand`. */
  readonly name: string;
  /** A share draws a bar in its period's colour; an amount is only words. */
  readonly kind: "share" | "amount";
  readonly period: Period;
  readonly fraction: number | null;
  /** `41%`, `$15.00 left`, `—`. */
  readonly value: string;
  readonly low: boolean;
  /** When it next changes, `in 6d 2h`, or a word on why there is no number. */
  readonly reset: string;
}

/** One account as the widget draws it. */
export interface WidgetAccount {
  readonly id: AccountId;
  readonly providerId: ProviderId;
  /** The provider, with the account's own name when there are two of them. */
  readonly name: string;
  /** The rings, outer first; empty for an account that has only money. */
  readonly rings: readonly WidgetRing[];
  /** The number under the rings, and the label that says which ring it is. */
  readonly headline: {
    readonly tag: string;
    readonly value: string;
    readonly low: boolean;
  };
  /**
   * Whether every ring's value is written under the tile. Not when something
   * is wrong with the account, which the headline says instead, and not when
   * there are no rings, as for an account that has only money.
   */
  readonly ringValues: boolean;
  /** Every limit: the rings' windows first, then money and other limits. */
  readonly rows: readonly WidgetRow[];
  /** The sentence an assistive technology reads for the whole account. */
  readonly description: string;
}

/**
 * The accounts the widget shows, in the overview's order, except that an
 * account with nothing to rank (only money, only unlimited, only native
 * units) follows the ranked ones: it needs no checking, so it is not first.
 */
export function widgetAccounts(
  accounts: readonly AccountSnapshot[],
  preferences: Preferences | null,
  now: number,
): readonly WidgetAccount[] {
  const placed = placeAccounts(accounts)
    .filter((entry) => entry.section !== "monitoring_off")
    .map((entry) => entry.account);
  const shown = [
    ...placed.filter((account) => !nothingToRank(account)),
    ...placed.filter(nothingToRank),
  ];
  return shown.map((account) => {
    const shared = shown.filter((other) => other.provider_id === account.provider_id);
    const provider = providerLabel(account.provider_id);
    const name =
      shared.length > 1
        ? `${provider} · ${displayName(preferences, accounts, account)}`
        : provider;
    return describe(account, name, now);
  });
}

/** Whether an account has no allowance to rank, which is not a problem. */
function nothingToRank(account: AccountSnapshot): boolean {
  return (
    account.order.kind === "unranked" &&
    UNRANKABLE.has(account.order.value.reason) &&
    !PROBLEMS.has(statusOf(account).text)
  );
}

/** The reasons an account has no rank that ask nothing of the person. */
const UNRANKABLE: ReadonlySet<string> = new Set([
  "no_included_allowance",
  "unlimited_only",
  "native_units_only",
]);

/** One account's rings, rows and headline. */
function describe(account: AccountSnapshot, name: string, now: number): WidgetAccount {
  const included = ordered(
    account.windows.filter((window) => window.metric_role === "included_allowance"),
  );
  const others = account.windows.filter(
    (window) => window.metric_role !== "included_allowance",
  );
  const rows = [...included, ...others].map((window) => row(account, window, now));
  const rings = ringsOf(rows.slice(0, included.length));
  const headline = headlineOf(account, rows, included.length, now);
  const words = rows.map((entry) => `${entry.name} ${entry.value}`).join(", ");
  const ringValues = rings.length > 0 && !PROBLEMS.has(statusOf(account, now).text);
  return {
    id: account.account_id,
    providerId: account.provider_id,
    name,
    rings,
    headline,
    ringValues,
    rows,
    description: words === "" ? `${name}: ${headline.value}` : `${name}: ${words}`,
  };
}

/** Included windows by period, the account's own before each model's. */
function ordered(windows: readonly QuotaWindow[]): readonly QuotaWindow[] {
  const accountFirst = (window: QuotaWindow): number =>
    window.scope.resource === ACCOUNT_RESOURCE ? 0 : 1;
  return [...windows].sort(
    (a, b) =>
      PERIODS.indexOf(a.category) - PERIODS.indexOf(b.category) ||
      accountFirst(a) - accountFirst(b),
  );
}

/** One window as a row. */
function row(account: AccountSnapshot, window: QuotaWindow, now: number): WidgetRow {
  const view = windowView(account, window, now);
  const known = viewKnown(view);
  const money = known ? moneyLeft(window.measurement) : null;
  const fraction = money === null ? viewFraction(view, window) : null;
  const percent = known ? remainingPercent(window.measurement) : null;
  return {
    tag: tagOf(window),
    name: windowLabel(window),
    kind: money === null && (fraction !== null || !known) ? "share" : "amount",
    period: window.category,
    fraction,
    value: money === null ? viewValue(view, window) : `${money} left`,
    low: money === null && percent !== null && percent <= LOW_PERCENT,
    reset: resetWords(view, window, now),
  };
}

/**
 * When a window next changes, in the fewest words the breakdown has room for.
 * A window that reports no change says nothing rather than a sentence.
 */
function resetWords(view: WindowView, window: QuotaWindow, now: number): string {
  switch (view) {
    case "current":
      return window.boundary === null
        ? ""
        : `in ${boundaryCountdown(window.boundary, now)}`;
    case "stale":
      return "last known";
    case "pending":
      return "verifying";
    case "unavailable":
      return "";
  }
}

/** The short label of a window. */
function tagOf(window: QuotaWindow): string {
  if (window.metric_role === "extra_spend_cap") {
    return "Extra";
  }
  if (window.metric_role === "credit_balance") {
    return "Credit";
  }
  if (window.scope.resource !== ACCOUNT_RESOURCE || window.category === "custom") {
    const word = window.scope.label.split(" ")[0] ?? "";
    return word === "" ? "Limit" : word.slice(0, 6);
  }
  switch (window.category) {
    case "session":
      return "5h";
    case "daily":
      return "Day";
    case "weekly":
      return "Wk";
    case "monthly":
      return "Mo";
  }
}

/**
 * One ring a period, outer first, so a single ring is always the outer one.
 * Two limits in one period, such as Claude's two weekly ones, share a ring,
 * which shows the tighter of the two.
 */
function ringsOf(rows: readonly WidgetRow[]): readonly WidgetRing[] {
  const rings: WidgetRing[] = [];
  for (const period of PERIODS) {
    const members = rows.filter((entry) => entry.period === period);
    if (members.length === 0 || rings.length === MAX_RINGS) {
      continue;
    }
    // The tighter of two limits in one period is the one the ring shows.
    const shown = members.reduce((tightest, entry) =>
      (entry.fraction ?? Infinity) < (tightest.fraction ?? Infinity) ? entry : tightest,
    );
    rings.push({
      period,
      fraction: shown.fraction,
      letter: LETTERS[period] ?? (shown.tag[0] ?? "").toUpperCase(),
      value: shown.value,
      low: shown.low,
    });
  }
  return rings;
}

/** The letter each standard period goes by under a tile. */
const LETTERS: Partial<Record<Period, string>> = {
  session: "H",
  daily: "D",
  weekly: "W",
  monthly: "M",
};

/**
 * The number under the rings: what is wrong with the account when something
 * is, else the tightest allowance, named so the number and its ring read
 * together, else the money an account without an allowance has left.
 */
function headlineOf(
  account: AccountSnapshot,
  rows: readonly WidgetRow[],
  includedCount: number,
  now: number,
): WidgetAccount["headline"] {
  const status = statusOf(account, now);
  if (PROBLEMS.has(status.text)) {
    return { tag: "", value: status.text, low: status.tone !== "pending" };
  }
  const shares = rows.slice(0, includedCount).filter((entry) => entry.fraction !== null);
  const tightest = shares.reduce<WidgetRow | null>(
    (lowest, entry) =>
      lowest === null || (entry.fraction ?? 1) < (lowest.fraction ?? 1) ? entry : lowest,
    null,
  );
  if (tightest !== null) {
    return { tag: tightest.tag, value: tightest.value, low: tightest.low };
  }
  const money = rows.find((entry) => entry.kind === "amount" && entry.value !== "—");
  if (money !== undefined) {
    return { tag: "", value: money.value.replace(/ left$/, ""), low: false };
  }
  return { tag: "", value: rows[0]?.value ?? "—", low: false };
}

/** The statuses that replace the number, because the number cannot be trusted. */
const PROBLEMS: ReadonlySet<string> = new Set([
  "Reconnect",
  "Connecting",
  "Rate limited",
  "Offline",
  "Check failed",
  "Paused",
]);
