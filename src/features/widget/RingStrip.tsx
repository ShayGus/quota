/**
 * The ring strip: one tile per account, up to five a row, rows as even as they
 * can be and centred.
 *
 * Under each tile's rings is the tightest allowance, named ("5h 41%") so the
 * number and its ring read together. A key under the tiles names the colours.
 * Pointing at a tile, or focusing it, opens a breakdown of every limit, outside
 * in, each with a small copy of the rings that lights the one it belongs to.
 * With two rows or more the breakdown covers the rows other than the tile's
 * own; with one row it opens under the tiles and the window grows to fit it.
 */
import { useEffect, useState, type CSSProperties, type JSX } from "react";

import type { AccountId } from "../../generated/bindings";
import { ProviderMark } from "../../shared/ui/ProviderMark";
import type { Period, WidgetAccount } from "./model";
import { periodClass, RADII, Rings } from "./Rings";

/** The most tiles in one row. */
const PER_ROW = 5;

/** A tile's height and the gap between rows, in CSS pixels. */
const TILE_HEIGHT = 66;
const ROW_GAP = 4;

/** Where the strip's first row starts, inside the widget's border. */
const FIRST_ROW_TOP = 9;

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
  onOpen,
}: {
  readonly accounts: readonly WidgetAccount[];
  readonly onOpen: (accountId: AccountId) => void;
}): JSX.Element {
  const [pointing, setPointing] = useState<number | null>(null);
  useClearWhenLeft(setPointing);
  const rows = Math.ceil(accounts.length / PER_ROW);
  const perRow = Math.ceil(accounts.length / rows);
  const target = pointing === null ? undefined : accounts[pointing];
  const overlay = target !== undefined && rows > 1;
  const inFlow = target !== undefined && rows === 1;
  return (
    <section
      className="widget-strip"
      aria-label="Quota"
      onPointerLeave={() => {
        setPointing(null);
      }}
    >
      <div className="widget-tiles" style={{ width: tilesWidth(perRow) }}>
        {accounts.map((account, index) => (
          <button
            key={account.id}
            type="button"
            className={`widget-tile${index === pointing ? " pointing" : ""}`}
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
              onOpen(account.id);
            }}
          >
            <span
              className={`widget-tile-rings logo-${String(Math.max(1, account.rings.length))}`}
            >
              <Rings rings={account.rings} size={44} stroke={3} />
              <ProviderMark providerId={account.providerId} />
            </span>
            <span className="widget-headline">
              {account.headline.tag === "" ? null : (
                <span className="widget-tag">{account.headline.tag}</span>
              )}
              <span className={`widget-value${account.headline.low ? " low" : ""}`}>
                {account.headline.value}
              </span>
            </span>
          </button>
        ))}
      </div>
      {inFlow ? (
        <Breakdown account={target} placement={null} />
      ) : (
        <Key periods={periodsOf(accounts)} />
      )}
      {overlay ? (
        <Breakdown
          account={target}
          placement={placement(Math.floor((pointing ?? 0) / perRow))}
        />
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
function tilesWidth(perRow: number): number {
  return perRow * 56 + (perRow - 1) * ROW_GAP;
}

/**
 * Where the overlay breakdown sits: over the rows below the first row when the
 * first row is pointed at, else over the rows above the pointed one.
 */
function placement(row: number): CSSProperties {
  if (row === 0) {
    return { top: FIRST_ROW_TOP + TILE_HEIGHT + 2, bottom: 3 };
  }
  return { top: 3, height: row * (TILE_HEIGHT + ROW_GAP) + 4 };
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
          <span className="widget-breakdown-reset">{row.reset}</span>
          <span className={`widget-value${row.low ? " low" : ""}`}>{row.value}</span>
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
