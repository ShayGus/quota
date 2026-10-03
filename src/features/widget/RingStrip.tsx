/**
 * The ring strip: one tile per account, up to four a row, rows as even as they
 * can be and centred.
 *
 * Under each tile's rings is every ring's value, outside in, each after the
 * letter of its period in the ring's colour: "H 99% W 50%". With up to three
 * tiles a row the tiles are wide and two values share a line; with four they
 * are narrow and each value has its own line. Every tile is as tall as the
 * tallest, so the rows stay even. A key under the tiles names the colours.
 * Pointing at a tile, clicking it, or focusing it, opens a breakdown of every limit, outside
 * in, each with a small copy of the rings that lights the one it belongs to.
 * With two rows or more the breakdown covers the rows other than the tile's
 * own; with one row it opens under the tiles and the window grows to fit it.
 */
import { useEffect, useState, type CSSProperties, type JSX } from "react";

import { ProviderMark } from "../../shared/ui/ProviderMark";
import type { Period, WidgetAccount } from "./model";
import { periodClass, RADII, Rings } from "./Rings";

/** The most tiles in one row. */
const PER_ROW = 4;

/** A tile's width with up to three a row, and with four, in CSS pixels. */
const WIDE_TILE = 96;
const NARROW_TILE = 72;

/** The gaps between tiles and between rows, in CSS pixels. */
const TILE_GAP = 2;
const ROW_GAP = 4;

/** A tile's height without its values, and the height of one line of them. */
const TILE_BASE_HEIGHT = 52;
const VALUE_LINE = 14;

/** Where the strip's first row starts, inside the widget's border. */
const FIRST_ROW_TOP = 9;

/** The key's height under the tiles. */
const KEY_HEIGHT = 14;

/** The overlay breakdown's padding, line height and line gap (see widget.css). */
const OVERLAY_PADDING = 5;
const OVERLAY_LINE = 13;
const OVERLAY_GAP = 3;

/** The names the key gives each period. */
const PERIOD_NAMES: Record<Period, string> = {
  session: "5-hour",
  daily: "Daily",
  weekly: "Weekly",
  monthly: "Monthly",
  custom: "Other",
};

/** The ring strip. */
export function RingStrip({
  accounts,
}: {
  readonly accounts: readonly WidgetAccount[];
}): JSX.Element {
  const [pointing, setPointing] = useState<number | null>(null);
  useClearWhenLeft(setPointing);
  const rows = Math.ceil(accounts.length / PER_ROW);
  const perRow = Math.ceil(accounts.length / rows);
  const wide = perRow < PER_ROW;
  const tileWidth = wide ? WIDE_TILE : NARROW_TILE;
  const tileHeight = tileHeightOf(accounts, wide ? 2 : 1);
  const target = pointing === null ? undefined : accounts[pointing];
  const pointedRow = Math.floor((pointing ?? 0) / perRow);
  const overlay =
    target !== undefined &&
    rows > 1 &&
    overlayFits(pointedRow, rows, tileHeight, target.rows.length);
  const inFlow = target !== undefined && !overlay;
  return (
    <section
      className="widget-strip"
      aria-label="Quota"
      onPointerLeave={() => {
        setPointing(null);
      }}
    >
      <div className="widget-tiles" style={{ width: tilesWidth(perRow, tileWidth) }}>
        {accounts.map((account, index) => (
          <button
            key={account.id}
            type="button"
            className={`widget-tile${index === pointing ? " pointing" : ""}`}
            style={{ width: tileWidth, height: tileHeight }}
            aria-label={account.description}
            onPointerEnter={() => {
              setPointing(index);
            }}
            onFocus={() => {
              setPointing(index);
            }}
            onBlur={() => {
              setPointing(null);
            }}
            onClick={() => {
              // A tap or a click opens the breakdown, as pointing does.
              setPointing(index);
            }}
          >
            <span
              className={`widget-tile-rings logo-${String(Math.max(1, account.rings.length))}`}
            >
              <Rings rings={account.rings} size={44} stroke={3} />
              <ProviderMark providerId={account.providerId} />
            </span>
            {account.ringValues ? (
              <span className="widget-readings">
                {account.rings.map((ring) => (
                  <span
                    key={ring.period}
                    className={`widget-reading ${periodClass(ring.period)}`}
                  >
                    <span className="widget-letter">{ring.letter}</span>
                    <span className={`widget-value${ring.low ? " low" : ""}`}>
                      {ring.value}
                    </span>
                  </span>
                ))}
              </span>
            ) : (
              <span className="widget-headline">
                {account.headline.tag === "" ? null : (
                  <span className="widget-tag">{account.headline.tag}</span>
                )}
                <span className={`widget-value${account.headline.low ? " low" : ""}`}>
                  {account.headline.value}
                </span>
              </span>
            )}
          </button>
        ))}
      </div>
      {inFlow ? (
        <Breakdown account={target} placement={null} />
      ) : (
        <Key periods={periodsOf(accounts)} />
      )}
      {overlay ? (
        <Breakdown account={target} placement={placement(pointedRow, tileHeight)} />
      ) : null}
    </section>
  );
}

/**
 * Closes the breakdown when the pointer leaves the window or the widget is
 * hidden, so it never reopens with the widget, still open from before.
 */
function useClearWhenLeft(setPointing: (pointing: null) => void): void {
  useEffect(() => {
    const clear = (): void => {
      setPointing(null);
    };
    const onVisibility = (): void => {
      if (document.visibilityState === "hidden") clear();
    };
    document.documentElement.addEventListener("mouseleave", clear);
    document.addEventListener("visibilitychange", onVisibility);
    window.addEventListener("blur", clear);
    return () => {
      document.documentElement.removeEventListener("mouseleave", clear);
      document.removeEventListener("visibilitychange", onVisibility);
      window.removeEventListener("blur", clear);
    };
  }, [setPointing]);
}

/** The tiles' row width, so rows wrap at the balanced count and centre. */
function tilesWidth(perRow: number, tileWidth: number): number {
  return perRow * tileWidth + (perRow - 1) * TILE_GAP;
}

/** Every tile's height: tall enough for the account with the most ring values. */
function tileHeightOf(accounts: readonly WidgetAccount[], perLine: number): number {
  const lines = Math.max(
    1,
    ...accounts.map((account) =>
      account.ringValues ? Math.ceil(account.rings.length / perLine) : 1,
    ),
  );
  return TILE_BASE_HEIGHT + lines * VALUE_LINE;
}

/**
 * Where the overlay breakdown sits: over the rows below the first row when the
 * first row is pointed at, else over the rows above the pointed one.
 */
function placement(row: number, tileHeight: number): CSSProperties {
  if (row === 0) {
    return { top: FIRST_ROW_TOP + tileHeight + 2, bottom: 3 };
  }
  return { top: 3, height: overlayRoom(row, 0, tileHeight) };
}

/** The height the overlay breakdown has over the other rows, in CSS pixels. */
function overlayRoom(row: number, rows: number, tileHeight: number): number {
  if (row > 0) {
    return row * (tileHeight + ROW_GAP) + 4;
  }
  const strip = 2 + 16 + rows * tileHeight + (rows - 1) * ROW_GAP + ROW_GAP + KEY_HEIGHT;
  return strip - 3 - (FIRST_ROW_TOP + tileHeight + 2);
}

/**
 * Whether the breakdown fits over the other rows. When it does not, as for an
 * account with many limits pointed at in a lower row, it opens under the tiles
 * and the window grows to fit it, so no limit is ever cut off.
 */
function overlayFits(
  row: number,
  rows: number,
  tileHeight: number,
  limits: number,
): boolean {
  const lines = limits + 1;
  const needed =
    2 * OVERLAY_PADDING + 2 + lines * OVERLAY_LINE + (lines - 1) * OVERLAY_GAP;
  return needed <= overlayRoom(row, rows, tileHeight);
}

/** The periods the strip draws, in ring order. */
function periodsOf(accounts: readonly WidgetAccount[]): readonly Period[] {
  const used = new Set(
    accounts.flatMap((account) => account.rings.map((ring) => ring.period)),
  );
  return (["session", "daily", "weekly", "monthly", "custom"] as const).filter((period) =>
    used.has(period),
  );
}

/** The key that names each ring colour. */
function Key({ periods }: { readonly periods: readonly Period[] }): JSX.Element | null {
  if (periods.length === 0) {
    return null;
  }
  return (
    <div className="widget-key" aria-hidden="true">
      {periods.map((period) => (
        <span key={period} className={periodClass(period)}>
          <span className="widget-key-ring" />
          {PERIOD_NAMES[period]}
        </span>
      ))}
    </div>
  );
}

/** Every limit of one account, outside in. */
function Breakdown({
  account,
  placement: overlay,
}: {
  readonly account: WidgetAccount;
  /** Where it covers the other rows, or `null` when it opens under the tiles. */
  readonly placement: CSSProperties | null;
}): JSX.Element {
  const ringPeriods = account.rings.map((ring) => ring.period);
  return (
    <div
      className={`widget-breakdown${overlay === null ? "" : " overlay"}`}
      role="tooltip"
      style={overlay ?? undefined}
    >
      <span className="widget-breakdown-name">{account.name}</span>
      {account.rows.map((row) => (
        <span key={`${row.name}:${row.tag}`} className="widget-breakdown-row">
          <RingMap
            periods={ringPeriods}
            lit={row.kind === "share" ? ringPeriods.indexOf(row.period) : -1}
          />
          <span className="widget-breakdown-label">{row.name}</span>
          {row.kind === "amount" ? null : (
            <span className="widget-breakdown-reset">{row.reset}</span>
          )}
          <span
            className={`widget-value${row.kind === "amount" ? " amount" : ""}${row.low ? " low" : ""}`}
          >
            {row.value}
          </span>
        </span>
      ))}
    </div>
  );
}

/** A tiny copy of an account's rings with one of them lit. */
function RingMap({
  periods,
  lit,
}: {
  readonly periods: readonly Period[];
  readonly lit: number;
}): JSX.Element {
  return (
    <svg
      className="widget-ring-map"
      width="14"
      height="14"
      viewBox="0 0 14 14"
      aria-hidden="true"
    >
      {lit === -1
        ? null
        : periods.map((period, layer) => (
            <circle
              key={period}
              className={layer === lit ? `${periodClass(period)} lit` : undefined}
              cx="7"
              cy="7"
              r={((RADII[layer] ?? 0) / 22) * 6.3}
            />
          ))}
    </svg>
  );
}
