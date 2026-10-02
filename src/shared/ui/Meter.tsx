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

/** A percentage label split into its number and its sign, as the wireframe sets it. */
const PERCENT = /^(<?\d+(?:\.\d+)?)%$/;

/** The value at a ring's centre: a large number with a small sign, or words. */
function RingValue({ label }: { readonly label: string }): JSX.Element {
  const percent = PERCENT.exec(label);
  if (percent !== null) {
    return (
      <span className="ring-value">
        {percent[1]}
        <span>%</span>
      </span>
    );
  }
  return <span className={`ring-value${label === "—" ? "" : " text"}`}>{label}</span>;
}

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
  /** The value text, already formatted. `—` when there is no reading. */
  readonly label: string;
  /** The small word under the value. */
  readonly caption: string;
}): JSX.Element {
  const known = fraction !== null;
  return (
    <div className={`ring ${severity}`} aria-hidden="true">
      <svg viewBox="0 0 100 100" fill="none">
        <circle className="ring-track" cx="50" cy="50" r={RADIUS} strokeWidth={STROKE} />
        <circle
          className="ring-arc"
          cx="50"
          cy="50"
          r={RADIUS}
          strokeWidth={STROKE}
          pathLength={100}
          strokeDasharray={known ? `${String(Math.round(fraction * 100))} 100` : "0 100"}
          strokeLinecap="butt"
          transform="rotate(-90 50 50)"
        />
      </svg>
      <div className="ring-center">
        <RingValue label={label} />
        <span className="ring-caption">{caption}</span>
      </div>
    </div>
  );
}

/** One horizontal bar, for the compact layout. */
export function Bar({ fraction }: { readonly fraction: number | null }): JSX.Element {
  return (
    <span className="bar-track">
      <span
        className="bar-fill"
        style={{ width: fraction === null ? "0%" : `${String(fraction * 100)}%` }}
      />
    </span>
  );
}
