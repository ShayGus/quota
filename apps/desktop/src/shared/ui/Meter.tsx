/**
 * The allowance indicators.
 *
 * Both indicators show the same value; neither invents a number for a reading
 * that has none. A ring with no arc is drawn with a dashed track and the text
 * `—`, so an unknown reading can never look like a full ring or a zero
 * (spec 3.4, spec 6, AC-07).
 */
import type { JSX } from "react";

import type { Severity } from "../format/allowance";

/** The circle geometry, on a 100-unit view box with the arc starting at twelve. */
const RADIUS = 42;
const STROKE = 7;

/** One ring, with its centre label supplied by the caller. */
export function Ring({
  fraction,
  severity,
  label,
  caption,
}: {
  /** The remaining fraction in `0..1`, or `null` when there is no reading. */
  readonly fraction: number | null;
  readonly severity: Severity;
  /** The value text, already formatted. */
  readonly label: string;
  /** The small word under the value. */
  readonly caption: string;
}): JSX.Element {
  const known = fraction !== null;
  return (
    <span className={`ring ring--${severity}`} aria-hidden="true">
      <svg viewBox="0 0 100 100" fill="none">
        <circle
          className="ring__track"
          cx="50"
          cy="50"
          r={RADIUS}
          strokeWidth={STROKE}
        />
        <circle
          className="ring__arc"
          cx="50"
          cy="50"
          r={RADIUS}
          strokeWidth={STROKE}
          pathLength={100}
          strokeDasharray={known ? `${Math.round(fraction * 100)} 100` : "0 100"}
          strokeLinecap="butt"
          transform="rotate(-90 50 50)"
        />
      </svg>
      <span className="ring__center">
        <span className="ring__value">{label}</span>
        <span className="ring__caption">{caption}</span>
      </span>
    </span>
  );
}

/** One horizontal bar, for the compact indicator style. */
export function Bar({
  fraction,
  severity,
}: {
  readonly fraction: number | null;
  readonly severity: Severity;
}): JSX.Element {
  return (
    <span className={`bar bar--${severity}`} aria-hidden="true">
      <span
        className="bar__fill"
        style={{ width: fraction === null ? "0%" : `${fraction * 100}%` }}
      />
    </span>
  );
}
