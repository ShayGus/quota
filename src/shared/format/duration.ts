/**
 * Duration and boundary formatting.
 *
 * A countdown shown on screen is a label. It never refills a quota, changes
 * freshness, changes rank, or schedules a request (spec 7.8).
 */
import type { Boundary, BoundaryKind } from "../../generated/bindings";

/** An instant as it crosses the wire: RFC 3339 UTC text. */
export type DateTime = string;

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/** The words for what happens at a boundary. A duration alone never decides this. */
const BOUNDARY_WORDS: Record<BoundaryKind, string> = {
  full_reset: "Resets",
  next_replenishment: "Next replenishment",
  billing_boundary: "Billing boundary",
  unknown: "Boundary",
};

/** The instant a wire timestamp names, or `null` when it is not a usable instant. */
export function instantOf(value: DateTime | null): number | null {
  if (value === null) {
    return null;
  }
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : null;
}

/**
 * Formats the time until an instant.
 *
 * A boundary that has already passed reads `due`. A negative countdown is never
 * displayed, including after a clock change or a sleep (AC-26).
 */
export function formatCountdown(target: number, now: number): string {
  const remaining = target - now;
  if (remaining <= 0) {
    return "due";
  }
  if (remaining < MINUTE_MS) {
    return "under 1m";
  }
  const days = Math.floor(remaining / DAY_MS);
  const hours = Math.floor((remaining % DAY_MS) / HOUR_MS);
  const minutes = Math.floor((remaining % HOUR_MS) / MINUTE_MS);
  if (days > 0) {
    return `${days}d ${hours}h`;
  }
  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  return `${minutes}m`;
}

/** How old a reading is, in words. */
export function formatAge(since: number, now: number): string {
  const age = now - since;
  if (age <= 0) {
    return "just now";
  }
  if (age < MINUTE_MS) {
    return "under a minute ago";
  }
  const minutes = Math.floor(age / MINUTE_MS);
  if (minutes < 60) {
    return `${minutes}m ago`;
  }
  const hours = Math.floor(age / HOUR_MS);
  if (hours < 24) {
    return `${hours}h ago`;
  }
  return `${Math.floor(age / DAY_MS)}d ago`;
}

/** The label shown above a countdown, from what the boundary actually means. */
export function boundaryLead(boundary: Boundary): string {
  return BOUNDARY_WORDS[boundary.kind];
}

/**
 * The countdown label for one boundary, or a statement that there is not one.
 *
 * No reset date is guessed when the provider did not report one (AC-14).
 */
export function formatBoundary(boundary: Boundary | null, now: number): string {
  if (boundary === null) {
    return "No reported reset";
  }
  const instant = instantOf(boundary.at);
  if (instant === null) {
    return "No reported reset";
  }
  const remaining = formatCountdown(instant, now);
  return remaining === "due"
    ? `${boundaryLead(boundary)} · due`
    : `${boundaryLead(boundary)} in ${remaining}`;
}

/** The exact boundary time, in a fixed zone, for the details surface. */
export function formatExactInstant(value: DateTime, timeZone: string): string {
  const instant = instantOf(value);
  if (instant === null) {
    return "Not reported";
  }
  return new Intl.DateTimeFormat("en-GB", {
    timeZone,
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(new Date(instant));
}
