/**
 * Freshness of one window reading.
 *
 * Connection state, fetch state, freshness, and quota severity are separate
 * dimensions (spec 6). This module answers only the freshness question, so a
 * stale reading never keeps a healthy "Current" appearance, while a known
 * window on a partially reported account keeps its own value (AC-15, AC-53).
 */
import type { AccountSnapshot, QuotaWindow } from "../../generated/bindings";
import { instantOf } from "../../shared/format/duration";

/** How current one window's reading is. */
export type ReadingState = "current" | "stale";

/**
 * Whether the account's own rank reason means its readings are no longer current.
 *
 * The backend already decided this; the renderer only repeats it, so the two can
 * never disagree about whether the value still describes the current period.
 *
 * `incomplete` is deliberately not stale: the known windows on a partially
 * reported account are still current, and only the missing ones say so
 * (AC-53). `reset_pending` is stale, because a reported boundary has passed and
 * the old value no longer describes this period (AC-11).
 */
function accountReadingsAreStale(account: AccountSnapshot): boolean {
  if (!account.monitoring_enabled) {
    return true;
  }
  // Every variant is named, so a new one added to the contract fails to compile
  // here rather than silently reading as current.
  switch (account.connection_state) {
    case "never_connected":
    case "connecting":
    case "reauthentication_required":
    case "unsupported":
    case "disconnected":
      return true;
    case "connected":
      break;
  }
  if (account.order.kind === "ranked") {
    return false;
  }
  switch (account.order.value.reason) {
    case "stale":
    case "reset_pending":
    case "monitoring_paused":
      return true;
    case "disabled":
    case "incomplete":
    case "native_units_only":
    case "no_included_allowance":
    case "reconnect_required":
    case "unlimited_only":
      return false;
  }
}

/** How current one window's reading is. */
export function readingState(
  account: AccountSnapshot,
  window: QuotaWindow,
  now: number,
): ReadingState {
  const deadline = instantOf(window.valid_until);
  if (deadline !== null && deadline <= now) {
    return "stale";
  }
  // A boundary that has passed means the period this reading described is over.
  // Provider windows normally carry no valid_until, so without this the ring
  // stays healthy while the countdown beside it already says the window is due.
  const boundary = window.boundary ? instantOf(window.boundary.at) : null;
  if (boundary !== null && boundary <= now) {
    return "stale";
  }
  return accountReadingsAreStale(account) ? "stale" : "current";
}

/** The small word under the ring, for a reading in this state. */
export function freshnessCaption(state: ReadingState): string {
  return state === "current" ? "left" : "last known";
}
