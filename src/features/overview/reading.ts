/**
 * What one window's reading looks like on screen.
 *
 * The card, the compact rows, and the detail surface all draw a window from
 * this one description, so a reading can never be current in one place and
 * stale in another (spec 6, AC-15). Nothing here invents a number for a
 * reading that has none (AC-07).
 */
import type { AccountSnapshot, QuotaWindow } from "../../generated/bindings";
import {
  arcFraction,
  formatRemaining,
  hasReading,
  severityOf,
  type Severity,
} from "../../shared/format/allowance";
import {
  boundaryCountdown,
  boundaryLead,
  formatBoundary,
  instantOf,
} from "../../shared/format/duration";
import { readingState } from "./freshness";

/** The standard periods a card draws as rings, in the order it draws them. */
const MAIN_CATEGORIES: readonly QuotaWindow["category"][] = [
  "session",
  "daily",
  "weekly",
  "monthly",
];

/**
 * How one window's reading stands.
 *
 * `pending` is a window whose reported boundary has passed: the old value no
 * longer describes the period and a fresh reading has not arrived (AC-11).
 * `unavailable` is a window with no reading at all, which is unknown, not 0%.
 */
export type WindowView = "current" | "stale" | "pending" | "unavailable";

/** The most rings a card draws; further limits are listed under it. */
const MAX_RINGS = 3;

/**
 * The label of one window: its period, named after the model when the allowance
 * covers one model rather than the account ("Fable weekly"), or the provider's
 * own name for a custom window.
 */
export function windowLabel(window: QuotaWindow): string {
  const period = periodLabel(window);
  return isModelAllowance(window)
    ? `${window.scope.label} ${period.toLowerCase()}`
    : period;
}

/** Whether a window is an included allowance for one model, such as Fable's. */
function isModelAllowance(window: QuotaWindow): boolean {
  return (
    window.metric_role === "included_allowance" &&
    window.category !== "custom" &&
    window.scope.resource !== ACCOUNT_RESOURCE &&
    window.scope.label !== ""
  );
}

/** The period one window covers, or the provider's own name for a custom one. */
function periodLabel(window: QuotaWindow): string {
  switch (window.category) {
    case "session":
      return "5-hour";
    case "daily":
      return "Daily";
    case "weekly":
      return "Weekly";
    case "monthly":
      return "Monthly";
    case "custom":
      return window.scope.label || "Allowance";
  }
}

/** The label of an additional window, which names its scope as well as its period. */
export function extraLabel(window: QuotaWindow): string {
  return window.scope.label || windowLabel(window);
}

/**
 * The resource every provider gives an account-wide allowance:
 * `quota_domain::quota::scope::ACCOUNT_RESOURCE` on the host.
 */
export const ACCOUNT_RESOURCE = "account";

/**
 * The windows a card draws as rings, and the ones it lists below them.
 *
 * The rings are the account-wide included allowance of each standard period,
 * read from the scope the provider reported, never guessed, followed by each
 * model's own allowance (named for its model) while there is room for three.
 * Every other window, such as an extra-spend cap, is an independent limit that
 * the card lists under its own scope name.
 */
export function cardWindows(account: AccountSnapshot): {
  readonly main: readonly QuotaWindow[];
  readonly extra: readonly QuotaWindow[];
} {
  const included = account.windows.filter(
    (window) => window.metric_role === "included_allowance",
  );
  const main: QuotaWindow[] = [];
  for (const category of MAIN_CATEGORIES) {
    const window = included.find(
      (candidate) =>
        candidate.category === category && candidate.scope.resource === ACCOUNT_RESOURCE,
    );
    if (window !== undefined) {
      main.push(window);
    }
  }
  // A model's own allowance, such as Fable's weekly limit, is a ring of its own
  // after the account's, named for the model, while there is room.
  for (const category of MAIN_CATEGORIES) {
    for (const window of included.filter(
      (candidate) => candidate.category === category && isModelAllowance(candidate),
    )) {
      if (main.length < MAX_RINGS) {
        main.push(window);
      }
    }
  }
  const extra = account.windows.filter((window) => !main.includes(window));
  return { main, extra };
}

/** How one window's reading stands right now. */
export function windowView(
  account: AccountSnapshot,
  window: QuotaWindow,
  now: number,
): WindowView {
  // A reason in words ("Not reported") is not a reading either, so it is drawn
  // as unknown rather than as a value with a "left" caption.
  if (
    window.measurement.kind === "unavailable" ||
    window.measurement.kind === "not_entitled" ||
    !hasReading(window.measurement)
  ) {
    return "unavailable";
  }
  const boundary = window.boundary === null ? null : instantOf(window.boundary.at);
  if (boundary !== null && boundary <= now) {
    return "pending";
  }
  return readingState(account, window, now);
}

/** The ring or bar appearance for a window in this state. */
export function viewSeverity(view: WindowView, window: QuotaWindow): Severity {
  switch (view) {
    case "current":
      return severityOf(window.measurement);
    case "stale":
      return "stale";
    case "pending":
    case "unavailable":
      return "pending";
  }
}

/** Whether the value is drawn. A pending or unavailable window shows `—`. */
export function viewKnown(view: WindowView): boolean {
  return view === "current" || view === "stale";
}

/** The value at the centre of a ring, or in a bar's value column. */
export function viewValue(view: WindowView, window: QuotaWindow): string {
  return viewKnown(view) ? formatRemaining(window.measurement) : "—";
}

/** The arc fraction to draw, or `null` for no arc. */
export function viewFraction(view: WindowView, window: QuotaWindow): number | null {
  return viewKnown(view) ? arcFraction(window.measurement) : null;
}

/** The small word under a ring's value. */
export function viewCaption(view: WindowView, window: QuotaWindow): string {
  switch (view) {
    case "stale":
      return "last known";
    case "pending":
      return "verifying";
    case "unavailable":
      return window.measurement.kind === "not_entitled" ? "not included" : "no reading";
    case "current":
      return window.measurement.kind === "unlimited" ? "unlimited" : "left";
  }
}

/** The sentence an assistive technology reads for one window's reading. */
export function readingText(view: WindowView, window: QuotaWindow): string {
  switch (view) {
    case "pending":
      return "Reset due; verifying";
    case "unavailable":
      return "No current reading";
    case "stale":
      return `${formatRemaining(window.measurement)} last known remaining`;
    case "current":
      return `${formatRemaining(window.measurement)} remaining`;
  }
}

/**
 * The line under a ring: when the window resets, or why that is not shown.
 *
 * `lead` and `time` are separate so the countdown can be set in bold.
 */
export function resetLine(
  view: WindowView,
  window: QuotaWindow,
  now: number,
): { readonly lead: string; readonly time: string | null } {
  switch (view) {
    case "pending":
      return { lead: "Reset due · verifying", time: null };
    case "unavailable":
      return { lead: "Not reported", time: null };
    case "stale":
      return { lead: "Last-known reading", time: null };
    case "current":
      return window.boundary === null
        ? { lead: "No reported reset", time: null }
        : {
            lead: `${boundaryLead(window.boundary)} in`,
            time: boundaryCountdown(window.boundary, now),
          };
  }
}

/** The short time column of a compact row. */
export function ledgerTime(view: WindowView, window: QuotaWindow, now: number): string {
  switch (view) {
    case "pending":
      return "Verifying";
    case "unavailable":
      return "Unknown";
    case "stale":
      return "Last known";
    case "current":
      return window.boundary === null ? "—" : boundaryCountdown(window.boundary, now);
  }
}

/** The sentence that says when a window's allowance changes, or why it is not shown. */
export function boundarySentence(
  view: WindowView,
  window: QuotaWindow,
  now: number,
): string {
  if (view === "current" && window.boundary !== null) {
    return formatBoundary(window.boundary, now);
  }
  return resetLine(view, window, now).lead;
}

/**
 * The accessible name of one window's control: whose allowance it is, which
 * window, the reading, and when it changes, so two accounts of one provider
 * never share a name.
 */
export function windowControlName(
  provider: string,
  accountLabel: string,
  label: string,
  view: WindowView,
  window: QuotaWindow,
  now: number,
): string {
  return `${provider} ${accountLabel}, ${label}: ${readingText(view, window)}. ${boundarySentence(view, window, now)}`;
}
