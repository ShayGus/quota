/**
 * Allowance formatting.
 *
 * One rule: a reading is either a number, or it is not. Nothing here turns a
 * missing reading into `0%` or into a full ring, because an unknown value is
 * not zero and not 100 (spec 3.4, spec 4.2, AC-08).
 */
import type { Measurement, Percent } from "../../generated/bindings";

/** How a reading should be drawn and labelled. */
export type Severity = "good" | "warn" | "danger" | "stale" | "pending";

/** A remaining percentage, or the fact that there is not one. */
export type Reading =
  | { readonly kind: "value"; readonly percent: Percent }
  | { readonly kind: "none"; readonly label: string };

/** The percentage remaining in one measurement, or `null` when there is none. */
export function remainingPercent(measurement: Measurement): number | null {
  switch (measurement.kind) {
    case "percentage":
      return measurement.value.remaining_percent;
    case "quantity": {
      const { remaining, limit } = measurement.value;
      return remaining !== null && limit !== null && limit > 0
        ? 100 * (remaining / limit)
        : null;
    }
    case "money": {
      const { remaining_minor_units: remaining, limit_minor_units: limit } = measurement.value;
      return remaining !== null && limit !== null && limit > 0
        ? 100 * (remaining / limit)
        : null;
    }
    case "unlimited":
    case "not_entitled":
    case "unavailable":
      return null;
  }
}

/**
 * The label for one reading.
 *
 * A positive remainder below one percent reads `<1%`, never `0%` (AC-08). A
 * genuine zero reads `0%`. An unavailable reading keeps its own words and never
 * acquires a percentage.
 */
export function formatRemaining(measurement: Measurement): string {
  if (measurement.kind === "unlimited") {
    return "Unlimited";
  }
  if (measurement.kind === "not_entitled") {
    return "Not included";
  }
  if (measurement.kind === "unavailable") {
    switch (measurement.value) {
      case "not_reported":
        return "Not reported";
      case "unsupported":
        return "Not supported";
      case "invalid_response":
        return "Unreadable";
      case "not_applicable":
        return "Not applicable";
    }
  }
  const percent = remainingPercent(measurement);
  if (percent === null) {
    return "No reading";
  }
  if (percent > 0 && percent < 1) {
    return "<1%";
  }
  const clamped = Math.min(100, Math.max(0, percent));
  const places = clamped <= 1 || clamped >= 99 ? 1 : 0;
  return `${clamped.toFixed(places)}%`;
}

/** The severity of a reading. Only a real number can be low or critical. */
export function severityOf(measurement: Measurement): Severity {
  const percent = remainingPercent(measurement);
  if (percent === null) {
    return "pending";
  }
  if (percent <= 10) {
    return "danger";
  }
  if (percent <= 20) {
    return "warn";
  }
  return "good";
}

/**
 * The arc fraction of a ring or the width of a bar, in `0..1`.
 *
 * `null` means "draw no arc": an unknown reading is not a full ring and not an
 * empty one (AC-07, spec 6).
 */
export function arcFraction(measurement: Measurement): number | null {
  const percent = remainingPercent(measurement);
  if (percent === null) {
    return null;
  }
  return Math.min(100, Math.max(0, percent)) / 100;
}
