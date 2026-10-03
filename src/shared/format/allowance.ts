/**
 * Allowance formatting.
 *
 * One rule: a reading is either a number, or it is not. Nothing here turns a
 * missing reading into `0%` or into a full ring, because an unknown value is
 * not zero and not 100 (spec 3.4, spec 4.2, AC-08).
 *
 * A count or an amount with no denominator is shown in its own unit and never
 * gains an invented percentage. An amount that has a denominator keeps both the
 * percentage and the amount (spec 5.2, AC-06).
 */
import type { Measurement, Percent, QuotaUnit } from "../../generated/bindings";

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
      const { remaining_minor_units: remaining, limit_minor_units: limit } =
        measurement.value;
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
 * The words for why a reading has no number.
 *
 * An unrecognized reason resolves to the general wording, so a payload this
 * build does not understand can never fall through into a numeric label.
 */
const UNAVAILABLE_WORDS: Record<string, string> = {
  not_reported: "Not reported",
  unsupported: "Not supported",
  invalid_response: "Unreadable",
  not_applicable: "Not applicable",
};

/** The words for the unit a counted allowance is expressed in. */
const UNIT_WORDS: Record<QuotaUnit["kind"], string> = {
  requests: "requests",
  tokens: "tokens",
  messages: "messages",
  credits: "credits",
  custom: "",
};

/**
 * The label for one reading.
 *
 * The forms are the ones the specification's own example table uses: a whole
 * number (`0%`, `53%`), `<1%` for a positive remainder below one percent so it
 * can never read as zero (AC-08), and one decimal place at the top of the range
 * so a near-full value cannot read as a false `100%`. A reading with no number
 * keeps words and never acquires a percentage (AC-07).
 *
 * A count or an amount with no percentage denominator is shown in its own unit
 * instead, because the amount is known even when the percentage is not
 * (spec 5.2, AC-06).
 */
export function formatRemaining(measurement: Measurement): string {
  if (measurement.kind === "unlimited") {
    return "Unlimited";
  }
  if (measurement.kind === "not_entitled") {
    return "Not included";
  }
  if (measurement.kind === "unavailable") {
    return UNAVAILABLE_WORDS[measurement.value] ?? "No reading";
  }
  const percent = remainingPercent(measurement);
  if (percent === null || !Number.isFinite(percent)) {
    return nativeAmount(measurement);
  }
  // A money amount keeps its own amount at every percentage, including none and
  // below one: the amount is known even when the percentage rounds to nothing,
  // and dropping it there would hide a cap's real remaining value (AC-06).
  const withAmount = (label: string): string => {
    if (measurement.kind !== "money") {
      return label;
    }
    const amount = nativeAmount(measurement);
    return amount === "No reading" || amount === label ? label : `${label} · ${amount}`;
  };
  if (percent <= 0) {
    return withAmount("0%");
  }
  if (percent < 1) {
    return withAmount("<1%");
  }
  if (percent >= 100) {
    return withAmount("100%");
  }
  // From 99 up one decimal shows, rounded down, so a near-full allowance never
  // reads as a false 100%: 99.97 is "99.9%". A decimal of zero is dropped, as it
  // is at 100: exactly 99 is "99%".
  return withAmount(
    `${percent < 99 ? percent.toFixed(0) : String(Math.floor(percent * 10) / 10)}%`,
  );
}

/**
 * The reading in its own unit, for a measurement that has no percentage.
 *
 * A count shows its count and its unit; an amount shows its value, its currency
 * and its scale. An amount whose percentages are known is still shown this way
 * when the percentage alone would hide it.
 */
function nativeAmount(measurement: Measurement): string {
  switch (measurement.kind) {
    case "percentage":
      return "No reading";
    case "quantity": {
      const { remaining, used } = measurement.value;
      const amount = remaining ?? used;
      if (amount === null) {
        return "No reading";
      }
      const { unit } = measurement.value;
      const label = unit.kind === "custom" ? unit.symbol : UNIT_WORDS[unit.kind];
      if (label.length === 0) {
        return "No reading";
      }
      return `${formatNumber(amount, measurement.value.precision)} ${label}`;
    }
    case "money": {
      const { currency, scale, remaining_minor_units: remaining } = measurement.value;
      if (remaining === null) {
        return "No reading";
      }
      return `${formatMoney(remaining, scale)} ${currency}`;
    }
    case "unlimited":
    case "not_entitled":
    case "unavailable":
      return "No reading";
  }
}

/**
 * A money amount from its minor units.
 *
 * Minor units are exact, so the amount is shown at the scale the provider gave
 * it. Displaying only the currency's usual two places would turn a real amount
 * into a zero, so a value that would read as zero keeps enough places to be
 * seen rather than disappearing.
 */
function formatMoney(minor: number, scale: number): string {
  const places = Number.isInteger(scale) && scale >= 0 ? Math.min(scale, 9) : 2;
  const value = minor / 10 ** places;
  const shown = value.toFixed(places);
  if (value !== 0 && Number.parseFloat(shown) === 0) {
    return value.toPrecision(3);
  }
  return shown;
}

/** One number, at the provider's own precision, from an untrusted payload. */
function formatNumber(value: number, precision: number): string {
  const places =
    Number.isInteger(precision) && precision >= 0 ? Math.min(precision, 9) : 0;
  return value.toFixed(places);
}

/**
 * Whether a measurement shows a value at all.
 *
 * A count or an amount with no denominator shows its own number, so a caption
 * under it must not claim there is no reading (spec 5.2, AC-06).
 */
export function hasReading(measurement: Measurement): boolean {
  return formatRemaining(measurement) !== "No reading";
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
