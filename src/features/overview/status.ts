/**
 * Status words for one account.
 *
 * Connection, fetch status, freshness, and quota severity are separate
 * dimensions (spec 6). This module names the single most useful one to show in
 * a row, and always says which window it is about.
 */
import type { AccountSnapshot, QuotaWindow } from "../../generated/bindings";
import { remainingPercent, type Severity } from "../../shared/format/allowance";
import { isRankable } from "../../shared/format/balance";
import { formatAge, instantOf } from "../../shared/format/duration";

/** A short status statement, its tone, and the icon that carries it. */
export interface StatusStatement {
  readonly text: string;
  readonly tone: Severity;
  readonly icon: "check" | "clock" | "link" | "pause" | "warning";
}

/** The window with the least remaining allowance that has a usable number. */
export function lowestWindow(account: AccountSnapshot): QuotaWindow | null {
  let lowest: QuotaWindow | null = null;
  let lowestValue = Number.POSITIVE_INFINITY;
  for (const window of account.windows) {
    // An extra-spend cap is a ceiling on spending beyond the plan, not an
    // allowance, so it never decides how close to exhaustion an account is.
    // A prepaid balance measured from its last top-up runs out like one.
    if (!isRankable(window)) {
      continue;
    }
    const percent = remainingPercent(window.measurement);
    if (percent !== null && percent < lowestValue) {
      lowest = window;
      lowestValue = percent;
    }
  }
  return lowest;
}

/** A short name for a window, for use inside a status phrase. */
export function shortName(window: QuotaWindow): string {
  switch (window.category) {
    case "session":
      return "5h";
    case "weekly":
      return "Weekly";
    case "monthly":
      return "Monthly";
    case "daily":
      return "Daily";
    case "custom":
      return window.scope.label.length > 0 ? window.scope.label : "Custom";
  }
}

/**
 * How long ago the last accepted reading arrived, in the badge's short form.
 *
 * `null` when there is no reading or it is under a minute old, because the
 * badge then says only that the reading is stale.
 */
function staleAge(account: AccountSnapshot, now: number | undefined): string | null {
  const since = instantOf(account.last_success_at);
  if (since === null || now === undefined || now - since < 60_000) {
    return null;
  }
  return formatAge(since, now).replace(/ ago$/, "");
}

/** The status of one account, and the window it is about, when there is one. */
export function statusOf(account: AccountSnapshot, now?: number): StatusStatement {
  if (!account.monitoring_enabled) {
    return { text: "Monitoring off", tone: "pending", icon: "pause" };
  }
  if (account.connection_state === "reauthentication_required") {
    return { text: "Reconnect", tone: "warn", icon: "link" };
  }
  if (account.connection_state === "disconnected") {
    return { text: "Disconnected", tone: "pending", icon: "pause" };
  }
  if (account.connection_state === "connecting") {
    return { text: "Connecting", tone: "pending", icon: "clock" };
  }
  if (account.fetch_state === "backoff") {
    return { text: "Rate limited", tone: "warn", icon: "clock" };
  }
  if (account.fetch_state === "offline") {
    return { text: "Offline", tone: "pending", icon: "clock" };
  }
  if (account.fetch_state === "error") {
    return { text: "Check failed", tone: "warn", icon: "warning" };
  }
  if (account.order.kind === "unranked") {
    switch (account.order.value.reason) {
      case "stale": {
        const age = staleAge(account, now);
        return {
          text: age === null ? "Stale" : `Stale · ${age}`,
          tone: "pending",
          icon: "clock",
        };
      }
      case "reset_pending":
        return { text: "Verifying reset", tone: "pending", icon: "clock" };
      case "incomplete":
        return { text: "Partially reported", tone: "warn", icon: "warning" };
      case "native_units_only":
        return { text: "Native units only", tone: "pending", icon: "check" };
      case "unlimited_only":
        return { text: "Unlimited", tone: "good", icon: "check" };
      case "no_included_allowance":
        return { text: "No included allowance", tone: "pending", icon: "check" };
      case "disabled":
        return { text: "Monitoring off", tone: "pending", icon: "pause" };
      case "monitoring_paused":
        return { text: "Paused", tone: "pending", icon: "pause" };
      case "reconnect_required":
        return { text: "Reconnect", tone: "warn", icon: "link" };
    }
  }
  const lowest = lowestWindow(account);
  if (lowest === null) {
    return { text: "Current", tone: "good", icon: "check" };
  }
  const percent = remainingPercent(lowest.measurement);
  const name = shortName(lowest);
  if (percent === null) {
    return { text: "Current", tone: "good", icon: "check" };
  }
  if (percent <= 0) {
    return { text: `${name} exhausted`, tone: "danger", icon: "warning" };
  }
  // The wireframe names a low window "low" at both thresholds; the tone carries
  // the difference between low and critical.
  if (percent <= 20) {
    return {
      text: `${name} low`,
      tone: percent <= 10 ? "danger" : "warn",
      icon: "warning",
    };
  }
  return { text: "Current", tone: "good", icon: "check" };
}

/**
 * Whether an account belongs in the needs-attention filter.
 *
 * An account whose monitoring is off, or a paused view, asks nothing of the
 * person, so neither counts; an unlimited allowance is not a problem either.
 */
export function needsAttention(account: AccountSnapshot): boolean {
  if (!account.monitoring_enabled) {
    return false;
  }
  if (
    account.order.kind === "unranked" &&
    account.order.value.reason === "monitoring_paused"
  ) {
    return false;
  }
  const { text } = statusOf(account);
  return text !== "Current" && text !== "Unlimited";
}
