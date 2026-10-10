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
  GroupSnapshot,
  Preferences,
  ProviderId,
  QuotaWindow,
} from "../../generated/bindings";
import { displayName } from "../../shared/format/alias";
import {
  accountView,
  groupLabel,
  groupReader,
  keyLimitOf,
  keyMonthSpend,
} from "../../shared/format/group";
import {
  moneyLeft,
  remainingPercent,
  type Severity,
} from "../../shared/format/allowance";
import { isPrepaidBalance, isRankable, isWindowShown } from "../../shared/format/balance";
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
import { statusOf, type StatusStatement } from "../overview/status";

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
  /** Whether the value is the last accepted reading, drawn muted. */
  readonly stale: boolean;
}

/** One account as the widget draws it. */
export interface WidgetAccount {
  /** The tile's identity: the account's, or `group:` and a group's for its total. */
  readonly id: AccountId;
  readonly providerId: ProviderId;
  /** The provider, with the account's own name when there are two of them. */
  readonly name: string;
  /**
   * The words inside the rings in place of the provider's mark: a group's
   * key is named there, because its group's tiles all share one mark.
   */
  readonly mark: string | null;
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
  /**
   * Whether the numbers shown are the last readings, kept through a passing
   * problem such as being offline, and drawn muted. The status itself is in
   * the peek and the drawer's chip.
   */
  readonly lastKnown: boolean;
  /** Every limit: the rings' windows first, then money and other limits. */
  readonly rows: readonly WidgetRow[];
  /** The sentence an assistive technology reads for the whole account. */
  readonly description: string;
  /**
   * The drawer's status chip, for an account whose number cannot be trusted.
   * `null` when the account reads normally and the drawer has no chip.
   */
  readonly chip: { readonly text: string; readonly tone: Severity } | null;
  /** The peek line's summary, after the account name: its status or reset. */
  readonly peek: string;
}

/**
 * The accounts the widget shows, in the overview's order, except that an
 * account with nothing to rank (only money, only unlimited, only native
 * units) follows the ranked ones: it needs no checking, so it is not first.
 *
 * A group of keys is a tile for the account's total, where its first key
 * falls, named for the group and drawn from the key that read the balance
 * last; then a tile for each key the person shows, its own spend limit.
 */
export function widgetAccounts(
  accounts: readonly AccountSnapshot[],
  preferences: Preferences | null,
  now: number,
  groups: readonly GroupSnapshot[] = [],
): readonly WidgetAccount[] {
  const placed = placeAccounts(accounts)
    .filter((entry) => entry.section !== "monitoring_off")
    .map((entry) => entry.account);
  const ordered = [
    ...placed.filter((account) => !nothingToRank(account)),
    ...placed.filter(nothingToRank),
  ];
  const tiles: WidgetAccount[] = [];
  const done = new Set<string>();
  for (const account of ordered) {
    const provider = providerLabel(account.provider_id);
    const group = groups.find((candidate) => candidate.id === account.group?.id);
    if (group === undefined) {
      const shared = ordered.filter((other) => other.provider_id === account.provider_id);
      const name =
        shared.length > 1
          ? `${provider} · ${displayName(preferences, accounts, account)}`
          : provider;
      tiles.push(describe(account, name, now, "whole"));
      continue;
    }
    if (done.has(group.id)) {
      continue;
    }
    done.add(group.id);
    const label = groupLabel(preferences, groups, group);
    const members = ordered.filter((member) => member.group?.id === group.id);
    const reader = groupReader(members) ?? account;
    tiles.push({
      ...describe(accountView(reader), `${provider} · ${label}`, now, "whole"),
      id: `group:${group.id}`,
    });
    for (const member of members.filter((entry) => entry.group?.key_shown !== false)) {
      const keyName = displayName(preferences, accounts, member);
      tiles.push({
        ...describe(member, `${label} · ${keyName}`, now, "key"),
        mark: keyMark(keyName),
      });
    }
  }
  return tiles;
}

/** The most letters of a key's name drawn inside its ring. */
const KEY_MARK_LENGTH = 5;

/** A key's name as its ring holds it: its first word, at most five letters. */
function keyMark(name: string): string {
  const word = name.trim().split(/\s+/)[0] ?? "";
  return word.slice(0, KEY_MARK_LENGTH);
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

/**
 * One account's rings, rows and headline. A group's key is drawn as its own
 * spend limit alone, as a ring, because that limit is what is the key's.
 */
function describe(
  account: AccountSnapshot,
  name: string,
  now: number,
  part: "whole" | "key",
): WidgetAccount {
  // A prepaid balance measured from its last top-up is drawn as a ring, like
  // an allowance; a key limit beside it only when the person chose to show it.
  const limit = part === "key" ? keyLimitOf(account) : null;
  const shown =
    part === "key"
      ? limit === null
        ? []
        : [limit]
      : account.windows.filter((window) => isWindowShown(account, window));
  const included = part === "key" ? shown : ordered(shown.filter(isRankable));
  const others = part === "key" ? [] : shown.filter((window) => !isRankable(window));
  const rows = [...included, ...others].map((window) =>
    row(account, window, now, part === "key"),
  );
  if (part === "key" && limit === null) {
    const spent = keyMonthSpend(account);
    rows.push({
      tag: "Spent",
      name: "Spent this month",
      kind: "amount",
      period: "monthly",
      fraction: null,
      value: spent ?? "—",
      low: false,
      reset: "no spend limit",
      stale: false,
    });
  }
  const rings = ringsOf(rows.slice(0, included.length));
  const status = statusOf(account, now);
  const problem = PROBLEMS.has(status.text)
    ? { text: status.text, tone: status.tone }
    : null;
  // A passing problem keeps the last readings on the tile, muted, as the full
  // window does; only an account with no reading, or one that must be
  // reconnected, shows the status instead of its numbers.
  const lastKnown =
    problem !== null &&
    !REPLACE_NUMBERS.has(problem.text) &&
    rows.some((entry) => entry.value !== "—");
  const replaced = problem !== null && !lastKnown;
  const { headline, row: headlineRow } = headlineOf(
    status,
    rows,
    included.length,
    replaced,
  );
  const words = rows.map((entry) => `${entry.name} ${entry.value}`).join(", ");
  const ringValues = rings.length > 0 && !replaced;
  return {
    id: account.account_id,
    providerId: account.provider_id,
    name,
    mark: null,
    rings,
    headline,
    ringValues,
    lastKnown,
    rows,
    description: words === "" ? `${name}: ${headline.value}` : `${name}: ${words}`,
    chip: problem,
    peek: peekSummary(problem, rows, headlineRow),
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

/**
 * One window as a row. A gauge, a prepaid balance or a key's own limit, is a
 * share drawn as a ring and shown as its money left.
 */
function row(
  account: AccountSnapshot,
  window: QuotaWindow,
  now: number,
  keyGauge = false,
): WidgetRow {
  const view = windowView(account, window, now);
  const known = viewKnown(view);
  // A prepaid balance is a share of its last top-up, shown as its money left.
  const gauge = keyGauge || isPrepaidBalance(window);
  const money = known ? moneyLeft(window.measurement) : null;
  const fraction = money === null || gauge ? viewFraction(view, window) : null;
  const percent = known ? remainingPercent(window.measurement) : null;
  const reset = resetWords(view, window, now);
  return {
    tag: keyGauge ? "Key" : tagOf(window),
    name: windowLabel(window),
    kind: gauge || (money === null && (fraction !== null || !known)) ? "share" : "amount",
    period: window.category,
    fraction,
    value: money === null ? viewValue(view, window) : gauge ? money : `${money} left`,
    low: (money === null || gauge) && percent !== null && percent <= LOW_PERCENT,
    reset,
    stale: reset === "last known",
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
  if (window.metric_role === "credit_balance" || isPrepaidBalance(window)) {
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
 * together, else the money an account without an allowance has left. The
 * headline's own row comes back with it, so the peek line can name the same
 * limit.
 */
function headlineOf(
  status: StatusStatement,
  rows: readonly WidgetRow[],
  includedCount: number,
  replaced: boolean,
): { readonly headline: WidgetAccount["headline"]; readonly row: WidgetRow | null } {
  if (replaced) {
    return {
      headline: { tag: "", value: status.text, low: status.tone !== "pending" },
      row: null,
    };
  }
  const shares = rows.slice(0, includedCount).filter((entry) => entry.fraction !== null);
  const tightest = shares.reduce<WidgetRow | null>(
    (lowest, entry) =>
      lowest === null || (entry.fraction ?? 1) < (lowest.fraction ?? 1) ? entry : lowest,
    null,
  );
  if (tightest !== null) {
    return {
      headline: { tag: tightest.tag, value: tightest.value, low: tightest.low },
      row: tightest,
    };
  }
  const money = rows.find((entry) => entry.kind === "amount" && entry.value !== "—");
  if (money !== undefined) {
    return {
      headline: { tag: "", value: money.value.replace(/ left$/, ""), low: false },
      row: money,
    };
  }
  const first = rows[0] ?? null;
  return {
    headline: { tag: "", value: first?.value ?? "—", low: false },
    row: first,
  };
}

/**
 * The peek line's summary for one account: a problem status with its
 * last-known note, else the headline row's reset, else its amount, else that
 * no reading was reported.
 */
function peekSummary(
  problem: { readonly text: string; readonly tone: Severity } | null,
  rows: readonly WidgetRow[],
  headlineRow: WidgetRow | null,
): string {
  if (problem !== null) {
    const lastKnown = rows.some((entry) => entry.stale);
    return lastKnown ? `${problem.text} · last known values` : problem.text;
  }
  if (headlineRow === null || headlineRow.value === "—") {
    return "no reading reported";
  }
  if (headlineRow.kind === "amount") {
    return headlineRow.value;
  }
  if (headlineRow.reset.startsWith("in ")) {
    return `${headlineRow.name} resets ${headlineRow.reset}`;
  }
  return headlineRow.reset === ""
    ? headlineRow.name
    : `${headlineRow.name} ${headlineRow.reset}`;
}

/**
 * The statuses that replace the number on the tile: no reading can be shown
 * for an account that must be reconnected. The others keep the last reading.
 */
const REPLACE_NUMBERS: ReadonlySet<string> = new Set(["Reconnect"]);

/** The statuses that say something is wrong with the account. */
const PROBLEMS: ReadonlySet<string> = new Set([
  "Reconnect",
  "Connecting",
  "Rate limited",
  "Offline",
  "Check failed",
  "Paused",
]);
