/**
 * Nested rings: one ring per period, the shortest outside.
 *
 * Each ring keeps its period's colour on every account. A ring with no reading
 * draws its track and no arc, because an unknown value is neither full nor
 * empty. An account with only money draws a dashed ring and no arc at all.
 */
import type { JSX } from "react";

import type { Period, WidgetRing } from "./model";

/** The radii of the outer, middle and inner ring in the 44-unit tile. */
export const RADII: readonly number[] = [19.5, 14.5, 9.5];

/** The rings of one account. */
export function Rings({
  rings,
  size,
  stroke,
}: {
  readonly rings: readonly WidgetRing[];
  readonly size: number;
  readonly stroke: number;
}): JSX.Element {
  const scale = size / 44;
  return (
    <svg
      className="widget-rings"
      width={size}
      height={size}
      viewBox={`0 0 ${String(size)} ${String(size)}`}
      aria-hidden="true"
    >
      {rings.length === 0 ? (
        <circle
          className="widget-ring-money"
          cx={size / 2}
          cy={size / 2}
          r={(RADII[0] ?? 0) * scale}
          strokeWidth={stroke}
        />
      ) : (
        rings.map((ring, layer) => (
          <Ring
            key={ring.period}
            ring={ring}
            radius={(RADII[layer] ?? 0) * scale}
            centre={size / 2}
            stroke={stroke}
          />
        ))
      )}
    </svg>
  );
}

function Ring({
  ring,
  radius,
  centre,
  stroke,
}: {
  readonly ring: WidgetRing;
  readonly radius: number;
  readonly centre: number;
  readonly stroke: number;
}): JSX.Element {
  const circumference = 2 * Math.PI * radius;
  return (
    <g className={periodClass(ring.period)}>
      <circle
        className="widget-ring-track"
        cx={centre}
        cy={centre}
        r={radius}
        strokeWidth={stroke}
      />
      {/* An exhausted ring draws no arc; a round cap alone would read as a sliver left. */}
      {ring.fraction === null || ring.fraction <= 0 ? null : (
        <circle
          className="widget-ring-arc"
          cx={centre}
          cy={centre}
          r={radius}
          strokeWidth={stroke}
          strokeDasharray={`${String(circumference * ring.fraction)} ${String(circumference)}`}
          transform={`rotate(-90 ${String(centre)} ${String(centre)})`}
        />
      )}
    </g>
  );
}

/** The class that gives an element its period's colour. */
export function periodClass(period: Period): string {
  return `period-${period}`;
}
