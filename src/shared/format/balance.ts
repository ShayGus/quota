/**
 * Words for a prepaid balance, measured from its last top-up.
 *
 * The host keeps the ledger (`quota_domain::balance`): what was loaded at the
 * last top-up, when, and how fast the balance is being spent. These turn it into
 * the lines a card and the detail show. Nothing here invents an amount; a value
 * the host did not give is left out.
 */
import type {
  AccountSnapshot,
  BalanceSummary,
  CreditKind,
  QuotaWindow,
} from "../../generated/bindings";
import { instantOf } from "./duration";

/** Whether a window is a prepaid balance measured from its last top-up. */
export function isPrepaidBalance(window: QuotaWindow): boolean {
  return window.metric_role === "prepaid_balance";
}

/**
 * Whether a window decides how close an account is to running out: an included
 * allowance, or a prepaid balance measured from its last top-up.
 */
export function isRankable(window: QuotaWindow): boolean {
  return window.metric_role === "included_allowance" || isPrepaidBalance(window);
}

/**
 * Whether a window is shown at all.
 *
 * An API key's spend limit beside a prepaid balance belongs to a key that often
 * exists only so Quota can read the account, so it is hidden until the person
 * turns it on for that account. Without a balance it is the only reading and is
 * always shown.
 */
export function isWindowShown(account: AccountSnapshot, window: QuotaWindow): boolean {
  if (window.metric_role !== "extra_spend_cap" || account.show_key_limit) {
    return true;
  }
  return !account.windows.some(isPrepaidBalance);
}

/** Whether an account has a key spend limit the person can choose to show. */
export function hasHideableKeyLimit(account: AccountSnapshot): boolean {
  return (
    account.windows.some(isPrepaidBalance) &&
    account.windows.some((window) => window.metric_role === "extra_spend_cap")
  );
}

/**
 * An amount from its minor units: `$37.20` for dollars, `37.20 EUR` otherwise.
 * `null` when the host sent no amount.
 */
export function formatAmount(
  minor: number | null,
  scale: number,
  currency: string,
): string | null {
  if (minor === null || !Number.isFinite(minor)) {
    return null;
  }
  const places = Number.isInteger(scale) && scale >= 0 ? Math.min(scale, 9) : 2;
  const sign = minor < 0 ? "-" : "";
  const amount = (Math.abs(minor) / 10 ** places).toFixed(places);
  return currency === "USD" ? `${sign}$${amount}` : `${sign}${amount} ${currency}`;
}

/** A day, as the card names it: `Oct 1`, in the computer's own time zone. */
export function formatDay(value: string): string | null {
  const instant = instantOf(value);
  if (instant === null) {
    return null;
  }
  return new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric" }).format(
    new Date(instant),
  );
}

/**
 * What the gauge is measured from: `of $50.00 loaded on Oct 1`, `of $40.00
 * since you added it`, or `of $65.00 since Oct 3` after a refund.
 */
export function baselineLine(balance: BalanceSummary): string | null {
  const amount = formatAmount(balance.baseline_minor, balance.scale, balance.currency);
  if (amount === null) {
    return null;
  }
  const day = formatDay(balance.baseline_at);
  switch (balance.baseline_kind) {
    case "top_up": {
      const loaded = balance.top_ups[0]?.amount_minor ?? null;
      const topUp = formatAmount(loaded, balance.scale, balance.currency);
      if (topUp === null || day === null) {
        return `of ${amount}`;
      }
      return topUp === amount
        ? `of ${amount} loaded on ${day}`
        : `of ${amount} after ${topUp} loaded on ${day}`;
    }
    case "since_added":
      return `of ${amount} since you added it`;
    case "adjusted":
      return day === null ? `of ${amount}` : `of ${amount} since ${day}`;
    case "credits":
      return `of ${amount} in active credits`;
  }
}

/**
 * The credit that expires first, for a provider that lists its grants:
 * `$4.20 expires Oct 31`. `null` without one.
 */
export function expiryLine(balance: BalanceSummary): string | null {
  const soonest = balance.credits[0];
  if (soonest === undefined) {
    return null;
  }
  const left = formatAmount(soonest.remaining_minor, balance.scale, balance.currency);
  const day = formatDay(soonest.expires_at);
  return left === null || day === null ? null : `${left} expires ${day}`;
}

/** What a credit grant was for, in words. */
export function creditKindLabel(kind: CreditKind): string {
  switch (kind) {
    case "free":
      return "Free credit";
    case "purchased":
      return "Purchased";
    case "other":
      return "Credit";
  }
}

/** How long the balance lasts: `≈ 12 days at $3.10/day`. `null` with no pace. */
export function runwayLine(balance: BalanceSummary): string | null {
  const { runway } = balance;
  if (runway === null) {
    return null;
  }
  const pace = formatAmount(runway.spend_per_day_minor, balance.scale, balance.currency);
  if (pace === null) {
    return null;
  }
  const days =
    runway.days_left === 0
      ? "under a day"
      : `≈ ${String(runway.days_left)} day${runway.days_left === 1 ? "" : "s"}`;
  return `${days} at ${pace}/day`;
}

/** The runway in the fewest words, for a compact row: `≈ 12 d`. */
export function runwayShort(balance: BalanceSummary | null): string | null {
  if (balance?.runway == null) {
    return null;
  }
  const { days_left: days } = balance.runway;
  return days === 0 ? "< 1 d" : `≈ ${String(days)} d`;
}
