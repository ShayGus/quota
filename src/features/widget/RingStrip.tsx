/**
 * The ring strip: one tile per account, up to four a row, rows as even as they
 * can be and centred.
 *
 * Under each tile's rings is every ring's value, outside in, each after the
 * letter of its period in the ring's colour: "H 99% W 50%". With up to three
 * tiles a row the tiles are wide and two values share a line; with four they
 * are narrow and each value has its own line. Every tile is as tall as the
 * tallest, so the rows stay even. A key under the tiles names the colours.
 *
 * Pointing at a tile only swaps the key line for a peek that names the
 * account and its status or next reset; the window never resizes for a peek.
 * A still click opens the account's details drawer under the tiles: the
 * window grows first and then the drawer unfolds. A press that moves 4 px or
 * more, anywhere on the widget, drags the window instead, and while a button
 * is held the peek, the drawer and every resize freeze. Near the screen's
 * bottom edge the drawer opens upward and the tiles keep their screen
 * position. The drawer closes on its tile, its close button, Esc, another
 * application taking focus, or its account leaving the widget.
 */
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type JSX,
} from "react";

import type { AccountId } from "../../generated/bindings";
import { launch } from "../../shared/ipc/report";
import type { WidgetGrowth } from "../../shared/ipc/widget";
import { Icon } from "../../shared/ui/Icon";
import { ProviderMark } from "../../shared/ui/ProviderMark";
import type { Period, WidgetAccount } from "./model";
import { periodClass, RADII, Rings } from "./Rings";
import { useWidgetChrome } from "./Widget";

/** The most tiles in one row. */
const PER_ROW = 4;

/** A tile's width with up to three a row, and with four, in CSS pixels. */
const WIDE_TILE = 92;
const NARROW_TILE = 68;

/** The gaps between tiles, between rows, and between the strip's blocks. */
const TILE_GAP = 8;
const ROW_GAP = 4;
const STRIP_GAP = 4;

/** A tile's height without its values, and the height of one line of them. */
const TILE_BASE_HEIGHT = 52;
const VALUE_LINE = 14;

/** The card's width, and the padding and border on both sides of it. */
const CARD_WIDTH = 316;
const CARD_CHROME = 18;

/** The footer line's height under the tiles. */
const FOOTER_HEIGHT = 14;

/**
 * The drawer's divider block, header, rows and pads, in CSS pixels
 * (widget.css draws with these same numbers).
 */
const DRAWER_DIVIDER = 16;
const DRAWER_HEADER = 20;
const DRAWER_ROWS_TOP = 6;
const DRAWER_ROW = 18;
const DRAWER_ROW_GAP = 2;
const DRAWER_ROWS_PAD = 2;
const DRAWER_PAD = 2;
/** Everything in the drawer but the rows themselves. */
const DRAWER_FIXED =
  DRAWER_DIVIDER + DRAWER_HEADER + DRAWER_ROWS_TOP + DRAWER_ROWS_PAD + DRAWER_PAD;

/** How long the peek lingers after the pointer leaves a tile. */
const PEEK_GRACE_MS = 120;
/** How long a lost focus waits for its return before the drawer closes. */
const BLUR_GRACE_MS = 150;
/** How long a close or shrink waits for its animation before giving up. */
const SETTLE_MS = 240;

/** The drawer and its heading, for the tiles' `aria-controls`. */
const DRAWER_ID = "widget-drawer";
const DRAWER_NAME_ID = "widget-drawer-name";

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
  const chrome = useWidgetChrome();
  const { requestFit, setDrawerOpen, setOnPressEnd } = chrome;
  const [hovered, setHovered] = useState<AccountId | null>(null);
  const [selected, setSelected] = useState<AccountId | null>(null);
  const [dir, setDir] = useState<WidgetGrowth>("down");
  const [fitted, setFitted] = useState(0);
  const [height, setHeight] = useState(0);
  const [closing, setClosing] = useState(false);
  const section = useRef<HTMLElement | null>(null);
  const drawer = useRef<HTMLDivElement | null>(null);
  const tiles = useRef(new Map<AccountId, HTMLButtonElement>());
  const hoverTimer = useRef<number | null>(null);
  const settleTimer = useRef<number | null>(null);
  const settleToken = useRef(0);
  const settleDone = useRef<(() => void) | null>(null);
  const keyboard = useRef(false);
  const rowsLength = useRef<number | null>(null);
  const rows = Math.ceil(accounts.length / PER_ROW);
  const perRow = Math.ceil(accounts.length / rows);
  const wide = perRow < PER_ROW;
  const tileWidth = wide ? WIDE_TILE : NARROW_TILE;
  const tileHeight = tileHeightOf(accounts, wide ? 2 : 1);
  const tilesWidth = perRow * tileWidth + (perRow - 1) * TILE_GAP;
  // One shared style object: an inline object in the mapped tiles keeps the
  // compiler from preserving the drawer callbacks below.
  const tileStyle = useMemo(
    () => ({ width: tileWidth, height: tileHeight }),
    [tileWidth, tileHeight],
  );
  const rest = restHeightOf(rows, tileHeight);
  const selectedAccount =
    selected === null ? undefined : accounts.find((entry) => entry.id === selected);
  const selectedIndex = accounts.findIndex((entry) => entry.id === selected);

  /**
   * Runs `run` when the drawer's height animation ends, or after a wait when
   * the animation never reports back, as in a test renderer.
   */
  const afterHeight = useCallback((run: () => void): void => {
    settleToken.current += 1;
    const token = settleToken.current;
    if (settleTimer.current !== null) {
      window.clearTimeout(settleTimer.current);
    }
    const done = (): void => {
      if (settleToken.current !== token || settleTimer.current === null) {
        return;
      }
      window.clearTimeout(settleTimer.current);
      settleTimer.current = null;
      settleDone.current = null;
      run();
    };
    settleDone.current = done;
    settleTimer.current = window.setTimeout(done, SETTLE_MS);
  }, []);

  const cancelSettle = useCallback((): void => {
    settleToken.current += 1;
    if (settleTimer.current !== null) {
      window.clearTimeout(settleTimer.current);
      settleTimer.current = null;
    }
    settleDone.current = null;
  }, []);

  const stopHoverTimer = useCallback((): void => {
    if (hoverTimer.current !== null) {
      window.clearTimeout(hoverTimer.current);
      hoverTimer.current = null;
    }
  }, []);

  useEffect(() => {
    setOnPressEnd((target): void => {
      stopHoverTimer();
      const hit = keyboard.current ? document.activeElement : target;
      const tile = [...tiles.current].find(
        ([, element]) => hit !== null && element.contains(hit),
      );
      setHovered(tile?.[0] ?? null);
    });
    return () => {
      setOnPressEnd(null);
    };
  }, [setOnPressEnd, stopHoverTimer]);

  /**
   * Shrinks the drawer to `want` first and the window after, for a switch or
   * a snapshot change to fewer rows.
   */
  const shrinkTo = useCallback(
    (id: AccountId, want: number): void => {
      setSelected(id);
      setClosing(false);
      setFitted(want);
      setHeight(want);
      afterHeight(() => {
        launch(requestFit(openHeightOf(rest, want), dir));
      });
    },
    [afterHeight, requestFit, rest, dir],
  );

  /** Opens the drawer for one account, or switches it there from another. */
  const open = (id: AccountId): void => {
    if (chrome.press.current !== null) {
      return;
    }
    const account = accounts.find((entry) => entry.id === id);
    if (account === undefined) {
      return;
    }
    cancelSettle();
    const want = drawerHeightOf(account.rows.length);
    if (selected !== null && selected !== id && want < fitted) {
      rowsLength.current = account.rows.length;
      shrinkTo(id, want);
      return;
    }
    launch(
      chrome
        .requestFit(openHeightOf(rest, want), selected === null ? "down" : dir)
        .then((answer) => {
          if (answer === null || answer.height === null) {
            return;
          }
          const grown = drawerOf(answer.height, rest);
          setSelected(id);
          setDir(answer.direction);
          setClosing(false);
          setFitted(grown);
          if (reducedMotion()) {
            setHeight(grown);
          }
          chrome.setDrawerOpen(true);
          rowsLength.current = account.rows.length;
        }),
    );
  };

  /** Closes the drawer: the drawer folds first and the window shrinks after. */
  const close = useCallback((): void => {
    if (selected === null || closing) {
      return;
    }
    cancelSettle();
    if (reducedMotion()) {
      launch(
        requestFit(rest, dir).then((answer) => {
          if (answer === null || answer.height === null) {
            return;
          }
          setSelected(null);
          setFitted(0);
          setHeight(0);
          setDrawerOpen(false);
        }),
      );
      return;
    }
    const grown = fitted;
    setClosing(true);
    setHeight(0);
    afterHeight(() => {
      launch(
        requestFit(rest, dir).then((answer) => {
          if (answer === null || answer.height === null) {
            // The window never shrank, so the drawer stays open.
            setClosing(false);
            setHeight(grown);
            return;
          }
          setSelected(null);
          setClosing(false);
          setFitted(0);
          setDrawerOpen(false);
        }),
      );
    });
  }, [
    selected,
    closing,
    fitted,
    rest,
    dir,
    requestFit,
    setDrawerOpen,
    cancelSettle,
    afterHeight,
  ]);

  // A fresh open mounts the drawer at no height; the current height is forced
  // into place first, so the next frame unfolds towards the target.
  useLayoutEffect(() => {
    if (selected === null || closing || height === fitted || reducedMotion()) {
      return;
    }
    drawer.current?.getBoundingClientRect();
    const frame = requestAnimationFrame(() => {
      setHeight(fitted);
    });
    return () => {
      cancelAnimationFrame(frame);
    };
  }, [selected, fitted, closing, height]);

  // A selected account that leaves the widget closes the drawer at once: its
  // content is gone, so there is nothing left to fold away.
  useEffect(() => {
    if (selected !== null && accounts.every((entry) => entry.id !== selected)) {
      cancelSettle();
      launch(
        chrome.requestFit(rest, dir).then(() => {
          setSelected(null);
          setClosing(false);
          setFitted(0);
          setHeight(0);
          chrome.setDrawerOpen(false);
        }),
      );
    }
  }, [selected, accounts, rest, dir, chrome, cancelSettle]);

  // Values update in place; when the row count changes the height follows,
  // growing the window first and shrinking it after.
  useEffect(() => {
    if (selected === null) {
      rowsLength.current = null;
      return;
    }
    const account = accounts.find((entry) => entry.id === selected);
    if (account === undefined) {
      return;
    }
    if (rowsLength.current === null) {
      rowsLength.current = account.rows.length;
      return;
    }
    if (rowsLength.current === account.rows.length || closing) {
      return;
    }
    rowsLength.current = account.rows.length;
    const want = drawerHeightOf(account.rows.length);
    const grown = fitted;
    const frame = requestAnimationFrame(() => {
      if (want >= grown) {
        launch(
          chrome.requestFit(openHeightOf(rest, want), dir).then((answer) => {
            if (answer === null || answer.height === null) {
              return;
            }
            setFitted(drawerOf(answer.height, rest));
          }),
        );
      } else {
        shrinkTo(selected, want);
      }
    });
    return () => {
      cancelAnimationFrame(frame);
    };
  }, [selected, accounts, closing, fitted, rest, dir, chrome, shrinkTo]);

  useEffect(() => {
    const onBlur = (): void => {
      window.setTimeout(() => {
        if (chrome.press.current !== null || document.hasFocus()) {
          return;
        }
        close();
      }, BLUR_GRACE_MS);
    };
    const onVisibility = (): void => {
      if (document.visibilityState === "hidden") {
        close();
      }
    };
    const onKey = (event: KeyboardEvent): void => {
      if (event.key !== "Escape" || selected === null) {
        return;
      }
      close();
      tiles.current.get(selected)?.focus();
    };
    window.addEventListener("blur", onBlur);
    document.addEventListener("visibilitychange", onVisibility);
    document.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("blur", onBlur);
      document.removeEventListener("visibilitychange", onVisibility);
      document.removeEventListener("keydown", onKey);
    };
  }, [chrome, close, selected]);

  // The corner buttons sit on the tiles, not on a drawer opened upward.
  useEffect(() => {
    const root = section.current?.closest(".widget");
    const card = root instanceof HTMLElement ? root : null;
    card?.style.setProperty(
      "--widget-drawer-above",
      selected !== null && dir === "up" ? `${String(fitted + STRIP_GAP)}px` : "0px",
    );
    return () => {
      card?.style.removeProperty("--widget-drawer-above");
    };
  }, [selected, dir, fitted]);

  useEffect(
    () => () => {
      if (hoverTimer.current !== null) {
        window.clearTimeout(hoverTimer.current);
      }
      if (settleTimer.current !== null) {
        window.clearTimeout(settleTimer.current);
      }
      chrome.setDrawerOpen(false);
    },
    [chrome],
  );

  return (
    <section
      className={`widget-strip${selected !== null && dir === "up" ? " up" : ""}`}
      aria-label="Quota"
      ref={section}
      onKeyDownCapture={() => {
        keyboard.current = true;
      }}
      onPointerDownCapture={() => {
        keyboard.current = false;
      }}
    >
      <div className="widget-tiles" style={{ width: tilesWidth }}>
        {accounts.map((account, index) => (
          <button
            key={account.id}
            ref={(tile) => {
              if (tile === null) {
                tiles.current.delete(account.id);
              } else {
                tiles.current.set(account.id, tile);
              }
            }}
            type="button"
            className={`widget-tile${account.id === hovered && account.id !== selected ? " hover" : ""}${account.id === selected ? " selected" : ""}${account.lastKnown ? " last-known" : ""}`}
            style={tileStyle}
            aria-label={account.description}
            aria-expanded={account.id === selected}
            aria-controls={DRAWER_ID}
            onPointerEnter={() => {
              if (chrome.press.current !== null) {
                return;
              }
              stopHoverTimer();
              setHovered(account.id);
            }}
            onPointerLeave={() => {
              if (chrome.press.current !== null) {
                return;
              }
              stopHoverTimer();
              hoverTimer.current = window.setTimeout(() => {
                setHovered(null);
              }, PEEK_GRACE_MS);
            }}
            onFocus={() => {
              if (keyboard.current) {
                stopHoverTimer();
                setHovered(account.id);
              }
            }}
            onBlur={(event) => {
              if (hovered === account.id && !event.currentTarget.matches(":hover")) {
                setHovered(null);
              }
            }}
            onClick={(event) => {
              // A second click of a double-click is ignored, so a
              // double-click never flashes the drawer open and closed.
              if (event.detail > 1) {
                return;
              }
              if (selected === account.id && !closing) {
                close();
              } else {
                open(account.id);
              }
            }}
            onKeyDown={(event) => {
              const count = accounts.length;
              let next: number | null = null;
              if (event.key === "ArrowRight") {
                next = Math.min(count - 1, index + 1);
              } else if (event.key === "ArrowLeft") {
                next = Math.max(0, index - 1);
              } else if (event.key === "ArrowDown") {
                next = Math.min(count - 1, index + perRow);
              } else if (event.key === "ArrowUp") {
                next = Math.max(0, index - perRow);
              } else if (event.key === "Home") {
                next = 0;
              } else if (event.key === "End") {
                next = count - 1;
              }
              if (next === null) {
                return;
              }
              event.preventDefault();
              const target = accounts[next];
              if (target !== undefined) {
                tiles.current.get(target.id)?.focus();
              }
            }}
          >
            <span
              className={`widget-tile-rings logo-${String(Math.max(1, account.rings.length))}`}
            >
              <Rings rings={account.rings} size={44} stroke={3} />
              <ProviderMark providerId={account.providerId} />
            </span>
            {account.ringValues ? (
              <span
                className={`widget-readings${wide ? "" : " stacked"}${account.lastKnown ? " last-known" : ""}`}
              >
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
              <span
                className={`widget-headline${account.lastKnown ? " last-known" : ""}`}
              >
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
      <Footer accounts={accounts} hovered={hovered} selected={selected} />
      {selectedAccount === undefined ? null : (
        <Drawer
          account={selectedAccount}
          caret={caretX(selectedIndex, perRow, tileWidth, tilesWidth)}
          grown={fitted}
          height={height}
          closing={closing}
          up={dir === "up"}
          drawerRef={drawer}
          onSettled={() => {
            settleDone.current?.();
          }}
          onClose={(fromKeyboard) => {
            close();
            if (fromKeyboard) {
              tiles.current.get(selectedAccount.id)?.focus();
            }
          }}
        />
      )}
    </section>
  );
}

/** Whether the drawer moves instantly instead of animating. */
function reducedMotion(): boolean {
  if (document.documentElement.dataset["reduceMotion"] === "true") {
    return true;
  }
  return (
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/** The window's height with the drawer closed. */
function restHeightOf(rows: number, tileHeight: number): number {
  return (
    CARD_CHROME + rows * tileHeight + (rows - 1) * ROW_GAP + STRIP_GAP + FOOTER_HEIGHT
  );
}

/** The drawer's height for an account with this many limits. */
function drawerHeightOf(limits: number): number {
  if (limits === 0) {
    return DRAWER_DIVIDER + DRAWER_HEADER + DRAWER_PAD;
  }
  return (
    DRAWER_DIVIDER +
    DRAWER_HEADER +
    DRAWER_ROWS_TOP +
    limits * DRAWER_ROW +
    (limits - 1) * DRAWER_ROW_GAP +
    DRAWER_ROWS_PAD +
    DRAWER_PAD
  );
}

/**
 * The window's height with a drawer of this height open: the closed height
 * plus the drawer and the flex gap between the footer and the drawer.
 */
function openHeightOf(rest: number, drawer: number): number {
  return rest + STRIP_GAP + drawer;
}

/** The drawer's height from a fitted window height. */
function drawerOf(total: number, rest: number): number {
  return Math.max(0, total - rest - STRIP_GAP);
}

/**
 * The caret's distance from the card's inner left edge: the selected tile's
 * centre, kept 14 px from either edge.
 */
function caretX(
  index: number,
  perRow: number,
  tileWidth: number,
  tilesWidth: number,
): number {
  const inner = CARD_WIDTH - CARD_CHROME;
  const centre =
    (inner - tilesWidth) / 2 + (index % perRow) * (tileWidth + TILE_GAP) + tileWidth / 2;
  return Math.min(Math.max(centre, 14), inner - 14);
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

/** The periods the strip draws, in ring order. */
function periodsOf(accounts: readonly WidgetAccount[]): readonly Period[] {
  const used = new Set(
    accounts.flatMap((account) => account.rings.map((ring) => ring.period)),
  );
  return (["session", "daily", "weekly", "monthly", "custom"] as const).filter((period) =>
    used.has(period),
  );
}

/**
 * The line under the tiles: the key at rest, the pointed account's peek on
 * hover or keyboard focus. It is always rendered, so the peek always has its
 * line.
 */
function Footer({
  accounts,
  hovered,
  selected,
}: {
  readonly accounts: readonly WidgetAccount[];
  readonly hovered: AccountId | null;
  readonly selected: AccountId | null;
}): JSX.Element {
  const target =
    hovered === null ? undefined : accounts.find((entry) => entry.id === hovered);
  return (
    <div className={`widget-foot${target === undefined ? "" : " peeking"}`}>
      <div className="widget-key">
        {periodsOf(accounts).map((period) => (
          <span key={period} className={periodClass(period)}>
            <span className="widget-key-ring" />
            {PERIOD_NAMES[period]}
          </span>
        ))}
      </div>
      <div className="widget-peek" aria-hidden="true">
        {target === undefined ? null : (
          <>
            <span className="widget-peek-in">
              <span className="widget-peek-name">{target.name}</span>
              {` · ${target.peek}`}
            </span>
            <Icon name={selected === target.id ? "chevron-up" : "chevron-down"} />
          </>
        )}
      </div>
    </div>
  );
}

/** One account's limits, unfolded on the host-confirmed side of the tiles. */
function Drawer({
  account,
  caret,
  grown,
  height,
  closing,
  up,
  drawerRef,
  onSettled,
  onClose,
}: {
  readonly account: WidgetAccount;
  /** The caret's distance from the card's inner left edge. */
  readonly caret: number;
  /** The drawer's fitted height, below its content when the room capped it. */
  readonly grown: number;
  /** The drawer's rendered height, which animates towards the fitted one. */
  readonly height: number;
  readonly closing: boolean;
  /** Whether the drawer unfolds above the tiles instead of below them. */
  readonly up: boolean;
  readonly drawerRef: { current: HTMLDivElement | null };
  /** Runs when the height animation ends. */
  readonly onSettled: () => void;
  /**
   * Closes the drawer. A keyboard close puts focus back on the tile, as Esc
   * does.
   */
  readonly onClose: (fromKeyboard: boolean) => void;
}): JSX.Element {
  const ringPeriods = account.rings.map((ring) => ring.period);
  const capped = grown + 0.5 < drawerHeightOf(account.rows.length);
  const rule = (
    <div className="widget-drawer-rule">
      <span className="widget-drawer-caret" style={{ left: caret }} />
    </div>
  );
  const body = (
    <>
      <div className="widget-drawer-head">
        <span className="widget-mark">
          <ProviderMark providerId={account.providerId} />
        </span>
        <span className="widget-drawer-name" id={DRAWER_NAME_ID}>
          {account.name}
        </span>
        {account.chip === null ? null : (
          <span className={`widget-chip${account.chip.tone === "warn" ? " warn" : ""}`}>
            <Icon name={account.chip.tone === "warn" ? "warning" : "clock"} />
            {account.chip.text}
          </span>
        )}
        <button
          type="button"
          className="widget-drawer-close"
          aria-label="Close the details"
          title="Close the details"
          onClick={(event) => {
            onClose(event.detail === 0);
          }}
        >
          <Icon name="close" />
        </button>
      </div>
      {account.rows.length === 0 ? null : (
        <div
          className={`widget-drawer-rows${capped ? " scroll" : ""}`}
          style={capped ? { maxHeight: Math.max(0, grown - DRAWER_FIXED) } : undefined}
        >
          {account.rows.map((row) => (
            <div className="widget-drawer-row" key={`${row.name}:${row.tag}`}>
              <RingMap
                periods={ringPeriods}
                lit={row.kind === "share" ? ringPeriods.indexOf(row.period) : -1}
              />
              <span className="widget-drawer-label">{row.name}</span>
              {row.kind === "amount" ? null : (
                <span className="widget-drawer-reset">{row.reset}</span>
              )}
              <span
                className={`widget-drawer-value${row.kind === "amount" ? " amount" : ""}${row.low ? " low" : ""}${row.stale ? " stale" : ""}`}
              >
                {row.value}
              </span>
            </div>
          ))}
        </div>
      )}
    </>
  );
  return (
    <div
      ref={drawerRef}
      id={DRAWER_ID}
      role="region"
      aria-labelledby={DRAWER_NAME_ID}
      className={`widget-drawer${closing ? " closing" : ""}`}
      style={{ height }}
      onTransitionEnd={(event) => {
        if (event.propertyName === "height") {
          onSettled();
        }
      }}
    >
      <div className="widget-drawer-in" key={account.id}>
        {up ? (
          <>
            {body}
            {rule}
          </>
        ) : (
          <>
            {rule}
            {body}
          </>
        )}
      </div>
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
