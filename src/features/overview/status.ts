/**
 * Status words for one account.
 *
 * Connection, fetch status, freshness, and quota severity are separate
 * dimensions (spec 6). This module names the single most useful one to show in
 * a row, and always says which window it is about.
 */
import type { AccountSnapshot, QuotaWindow } from "../../generated/bindings";
import { remainingPercent, type Severity } from "../../shared/format/allowance";

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
    if (window.metric_role !== "included_allowance") {
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
function shortName(window: QuotaWindow): string {
  switch (window.category) {
    case "session":
      return "Session";
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

/** The status of one account, and the window it is about, when there is one. */
export function statusOf(account: AccountSnapshot): StatusStatement {
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
      case "stale":
        return { text: "Stale reading", tone: "pending", icon: "clock" };
      case "reset_pending":
        return { text: "Reset pending", tone: "pending", icon: "clock" };
      case "incomplete":
        return { text: "Partial reading", tone: "warn", icon: "warning" };
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
    return { text: `${name} empty`, tone: "danger", icon: "warning" };
  }
  if (percent <= 10) {
    return { text: `${name} critical`, tone: "danger", icon: "warning" };
  }
  if (percent <= 20) {
    return { text: `${name} low`, tone: "warn", icon: "warning" };
  }
  return { text: "Current", tone: "good", icon: "check" };
}

/** Whether an account belongs in the needs-attention filter. */
export function needsAttention(account: AccountSnapshot): boolean {
  return statusOf(account).text !== "Current";
}
