/**
 * The mini widget: every account at a glance, in a small window that floats
 * above the others.
 *
 * It has two looks, picked with the same ring-or-bar choice as the overview:
 * a strip of rings, one ring per period, or a compact list, one row per account.
 * It is another way to present the app, never shown beside the full window.
 * The host fits the window to it, and it moves wherever it is dragged, from any
 * point on it. Its background is see-through until it is pointed at, while its
 * numbers and its rim never fade. The rim turns amber while a limit is low, and
 * the widget flashes when one runs low or when the person asks to find it. Each
 * look's details have a Reconnect button that reconnects an account the way the
 * full window does. The button in its corner, the tray menu, and Settings switch
 * back to the full window.
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type JSX,
} from "react";

import type { AccountId } from "../../generated/bindings";
import { launch } from "../../shared/ipc/report";
import {
  dragWidget,
  fitWidget,
  listenForFind,
  setContentWidth,
  showWidgetHalo,
  type FitWidget,
  type WidgetGrowth,
} from "../../shared/ipc/widget";
import type { RendererState } from "../../shared/state/types";
import { Icon } from "../../shared/ui/Icon";
import { useNow } from "../../shared/ui/useNow";
import { CompactList } from "./CompactList";
import { widgetAccounts, type WidgetAccount } from "./model";
import { RingStrip } from "./RingStrip";

/** How far the pointer moves before a press becomes a drag, in CSS pixels. */
const DRAG_DISTANCE = 4;

/** How long the pointer must rest on the widget before it turns solid. */
const SOLID_AFTER_MS = 90;
/** How long it stays solid after the pointer leaves. */
const SEE_THROUGH_AFTER_MS = 280;
/** How long a release waits for its own click before making one. */
const CLICK_GRACE_MS = 40;
/** How long a flash lasts, as `widget-rim-flash` in widget.css animates it. */
const FLASH_MS = 2700;

/** A press on the widget that may still become a drag. */
export interface WidgetPress {
  readonly pointerId: number;
  readonly x: number;
  readonly y: number;
  /** Whether the press already moved past the drag threshold. */
  dragging: boolean;
}

/**
 * What the ring strip needs from the widget window: the press a drag may
 * grow from, whether the drawer owns the window size, and the one way to ask
 * the host for a size. The strip reads the press to freeze while one is held.
 */
export interface WidgetChrome {
  readonly setOnPressEnd: (callback: ((target: Element | null) => void) | null) => void;
  /** The press in progress, or `null` when no button is held. */
  readonly press: { current: WidgetPress | null };
  /** Whether the drawer is open; the window fit then follows the drawer. */
  readonly drawerOpen: { current: boolean };
  /** Tells the window fit whether the drawer owns the window size. */
  readonly setDrawerOpen: (open: boolean) => void;
  /** The side the window's next fit prefers, which the look decides. */
  readonly growth: { current: WidgetGrowth };
  /** Tells the look what the host did with a fit it didn't ask for itself. */
  readonly setOnFitted: (
    callback: ((answer: FitWidget, asked: number) => void) | null,
  ) => void;
  /** The last height asked of the host, so a settling resize is not re-asked. */
  readonly target: { current: number | null };
  /**
   * Asks the host to fit the window to `contentHeight`. While a press is
   * held the ask waits and is sent when the press ends.
   */
  readonly requestFit: (
    contentHeight: number,
    direction: WidgetGrowth,
  ) => Promise<FitWidget | null>;
}

/** The press the drag hook reads and ends, owned by the window's chrome. */
interface WidgetPressControls {
  readonly press: { current: WidgetPress | null };
  readonly beginPress: (press: WidgetPress) => void;
  readonly markDragging: () => void;
  readonly endPress: (target: Element | null) => void;
}

const defaultDrawerOpen = { current: false };
const defaultOnPressEnd = { current: null as ((target: Element | null) => void) | null };
const defaultGrowth: { current: WidgetGrowth } = { current: "down" };

const WidgetChromeContext = createContext<WidgetChrome>({
  setOnPressEnd: (callback) => {
    defaultOnPressEnd.current = callback;
  },
  press: { current: null },
  drawerOpen: defaultDrawerOpen,
  setDrawerOpen: (open) => {
    defaultDrawerOpen.current = open;
  },
  growth: defaultGrowth,
  setOnFitted: () => undefined,
  target: { current: null },
  requestFit: (contentHeight, direction) => fitWidget(contentHeight, direction),
});

/** The widget window's chrome, or the standalone default outside the window. */
export function useWidgetChrome(): WidgetChrome {
  return useContext(WidgetChromeContext);
}

/** A fit asked while a press was held, sent when the press ends. */
interface PendingFit {
  readonly contentHeight: number;
  readonly direction: WidgetGrowth;
  readonly resolve: (fitted: FitWidget | null) => void;
}

/** The widget window's content. */
export function Widget({
  state,
  onExpand,
  onReconnect,
}: {
  readonly state: RendererState;
  /** Switches back to the full window. */
  readonly onExpand: () => void;
  /**
   * Reconnects one account, as the full window's Reconnect does. Answers the
   * words for why the host refused, or `null` once it took the reconnect.
   */
  readonly onReconnect: (accountId: AccountId) => Promise<string | null>;
}): JSX.Element {
  const now = useNow();
  const accounts = useMemo(
    () =>
      widgetAccounts(
        state.snapshot?.accounts ?? [],
        state.preferences,
        now,
        state.snapshot?.groups ?? [],
      ),
    [state.snapshot, state.preferences, now],
  );
  const root = useRef<HTMLDivElement | null>(null);
  const chrome = useWidgetChromeValue();
  useFitWindow(root, chrome);
  useTransparentPage();
  useDragToMove(chrome);
  const pointer = useSolidWhilePointedAt();
  const { flashing, low } = useFlash(accounts);
  return (
    <WidgetChromeContext value={chrome}>
      <div
        className={`widget${pointer.solid ? " solid" : ""}${flashing ? " flashing" : ""}${low ? " low" : ""}`}
        ref={root}
        onPointerEnter={pointer.onEnter}
        onPointerLeave={pointer.onLeave}
      >
        {accounts.length === 0 ? (
          <button type="button" className="widget-empty" onClick={onExpand}>
            <strong>No accounts to show</strong>
            <span>Open the full window to add one</span>
          </button>
        ) : state.preferences?.indicator_style === "bar" ? (
          <CompactList
            accounts={accounts}
            onReconnect={onReconnect}
            onExpand={onExpand}
          />
        ) : (
          <RingStrip accounts={accounts} onReconnect={onReconnect} />
        )}
        <button
          type="button"
          className="widget-expand"
          aria-label="Open the full window"
          title="Open the full window"
          onClick={onExpand}
        >
          <Icon name="expand" />
        </button>
      </div>
    </WidgetChromeContext>
  );
}

/** The look's answer to a fit the window asked for itself. */
interface WidgetFitAnswers {
  readonly onFitted: {
    readonly current: ((answer: FitWidget, asked: number) => void) | null;
  };
}

/** The window's shared press, drawer flag and deferred fit. */
function useWidgetChromeValue(): WidgetChrome & WidgetPressControls & WidgetFitAnswers {
  const onPressEnd = useRef<((target: Element | null) => void) | null>(null);
  const setOnPressEnd = useCallback(
    (callback: ((target: Element | null) => void) | null): void => {
      onPressEnd.current = callback;
    },
    [],
  );
  const press = useRef<WidgetPress | null>(null);
  const drawerOpen = useRef(false);
  const target = useRef<number | null>(null);
  const growth = useRef<WidgetGrowth>("down");
  const onFitted = useRef<((answer: FitWidget, asked: number) => void) | null>(null);
  const setOnFitted = useCallback(
    (callback: ((answer: FitWidget, asked: number) => void) | null): void => {
      onFitted.current = callback;
    },
    [],
  );
  const pending = useRef<PendingFit | null>(null);
  const send = useCallback(
    (contentHeight: number, direction: WidgetGrowth): Promise<FitWidget | null> => {
      target.current = contentHeight;
      return fitWidget(contentHeight, direction);
    },
    [],
  );
  const requestFit = useCallback(
    (contentHeight: number, direction: WidgetGrowth): Promise<FitWidget | null> => {
      const superseded = pending.current;
      pending.current = null;
      superseded?.resolve(null);
      if (press.current !== null) {
        const { promise, resolve } = Promise.withResolvers<FitWidget | null>();
        pending.current = { contentHeight, direction, resolve };
        return promise;
      }
      return send(contentHeight, direction);
    },
    [send],
  );
  const endPress = useCallback(
    (element: Element | null): void => {
      const heldPress = press.current;
      press.current = null;
      if (heldPress !== null) {
        onPressEnd.current?.(element);
      }
      const held = pending.current;
      pending.current = null;
      if (held !== null) {
        send(held.contentHeight, held.direction).then(held.resolve, held.resolve);
      }
    },
    [send],
  );
  const beginPress = useCallback(
    (next: WidgetPress): void => {
      press.current = next;
    },
    [press],
  );
  const markDragging = useCallback((): void => {
    const held = press.current;
    if (held !== null) {
      held.dragging = true;
    }
  }, [press]);
  const setDrawerOpen = useCallback(
    (open: boolean): void => {
      drawerOpen.current = open;
    },
    [drawerOpen],
  );
  return useMemo(
    () => ({
      setOnPressEnd,
      press,
      drawerOpen,
      setDrawerOpen,
      growth,
      setOnFitted,
      onFitted,
      target,
      requestFit,
      beginPress,
      markDragging,
      endPress,
    }),
    [
      requestFit,
      endPress,
      beginPress,
      markDragging,
      setDrawerOpen,
      setOnPressEnd,
      setOnFitted,
    ],
  );
}

/**
 * Whether the widget is solid. It stays see-through while the pointer only
 * passes over it, and a moment after the pointer leaves, so it doesn't flicker.
 */
function useSolidWhilePointedAt(): {
  readonly solid: boolean;
  readonly onEnter: () => void;
  readonly onLeave: () => void;
} {
  const [solid, setSolid] = useState(false);
  const timer = useRef<number | null>(null);
  const after = useCallback((next: boolean, wait: number): void => {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
    }
    timer.current = window.setTimeout(() => {
      timer.current = null;
      setSolid(next);
    }, wait);
  }, []);
  useEffect(
    () => () => {
      if (timer.current !== null) {
        window.clearTimeout(timer.current);
      }
    },
    [],
  );
  const onEnter = useCallback((): void => {
    after(true, SOLID_AFTER_MS);
  }, [after]);
  const onLeave = useCallback((): void => {
    after(false, SEE_THROUGH_AFTER_MS);
  }, [after]);
  return { solid, onEnter, onLeave };
}

/** The limits drawn low right now, each named by its account and its own name. */
function lowLimitsOf(accounts: readonly WidgetAccount[]): readonly string[] {
  return accounts
    .filter((account) => !account.lastKnown)
    .flatMap((account) =>
      account.rows.filter((row) => row.low).map((row) => `${account.id}:${row.name}`),
    );
}

/**
 * Whether the widget is flashing, and whether a limit is low. It flashes when
 * the host asks, and when a limit runs low: not for one that was already low
 * when the widget opened, which only colours the rim.
 */
function useFlash(accounts: readonly WidgetAccount[]): {
  readonly flashing: boolean;
  readonly low: boolean;
} {
  const lowNow = lowLimitsOf(accounts);
  const key = lowNow.join("|");
  const [flashes, setFlashes] = useState(0);
  const [finished, setFinished] = useState(0);
  const [ranLow, setRanLow] = useState(0);
  // Nothing is known until the first accounts arrive.
  const [seen, setSeen] = useState<string | null>(null);
  if (accounts.length > 0 && key !== seen) {
    setSeen(key);
    const before = new Set(seen === null ? lowNow : seen.split("|"));
    if (lowNow.some((limit) => !before.has(limit))) {
      setFlashes((count) => count + 1);
      setRanLow((count) => count + 1);
    }
  }
  useEffect(() => {
    let current = true;
    let stop: (() => void) | null = null;
    launch(
      listenForFind(() => {
        setFlashes((count) => count + 1);
      }).then((release) => {
        if (current) {
          stop = release;
        } else {
          release();
        }
      }),
    );
    return () => {
      current = false;
      stop?.();
    };
  }, []);
  useEffect(() => {
    if (ranLow > 0) {
      launch(showWidgetHalo());
    }
  }, [ranLow]);
  useEffect(() => {
    if (flashes === 0) {
      return;
    }
    const timer = window.setTimeout(() => {
      setFinished(flashes);
    }, FLASH_MS);
    return () => {
      window.clearTimeout(timer);
    };
  }, [flashes]);
  return { flashing: flashes !== finished, low: lowNow.length > 0 };
}

/**
 * Observes the resting content, height and width. While the ring strip's drawer
 * is open it fits explicitly around its animation, so intermediate heights are
 * not sent.
 */
function useFitWindow(
  root: { readonly current: HTMLElement | null },
  chrome: WidgetChrome & WidgetFitAnswers,
): void {
  const { drawerOpen, target, requestFit, growth, onFitted } = chrome;
  useEffect(() => {
    const element = root.current;
    if (element === null) {
      return;
    }
    let width: number | null = null;
    const fit = (): void => {
      const measured = Math.ceil(element.getBoundingClientRect().width);
      const height = element.offsetHeight;
      if (drawerOpen.current || (height === target.current && measured === width)) {
        return;
      }
      width = measured;
      setContentWidth(measured);
      launch(
        requestFit(height, growth.current).then((answer) => {
          if (answer !== null) {
            onFitted.current?.(answer, height);
          }
        }),
      );
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [root, drawerOpen, target, requestFit, growth, onFitted]);
}

/** Lets the desktop show through around the widget's rounded corners. */
function useTransparentPage(): void {
  useEffect(() => {
    document.documentElement.classList.add("widget-window");
    return () => {
      document.documentElement.classList.remove("widget-window");
    };
  }, []);
}

/**
 * Moves the window when it is dragged from anywhere on it, while a press that
 * does not move stays a click on whatever is under it.
 */
function useDragToMove(controls: WidgetPressControls): void {
  const { press, beginPress, markDragging, endPress } = controls;
  const suppressClick = useRef(false);
  const draggedBefore = useRef(false);
  const clickSeen = useRef(false);
  useEffect(() => {
    const finish = (event?: Event): void => {
      window.removeEventListener("pointermove", onMove, true);
      window.removeEventListener("pointerup", onRelease, true);
      window.removeEventListener("pointercancel", onRelease, true);
      window.removeEventListener("blur", finish);
      endPress(
        event instanceof window.PointerEvent && event.type !== "pointercancel"
          ? document.elementFromPoint(event.clientX, event.clientY)
          : null,
      );
    };
    const onMove = (event: globalThis.PointerEvent): void => {
      const held = press.current;
      if (held === null || event.pointerId !== held.pointerId) {
        return;
      }
      if ((event.buttons & 1) === 0) {
        finish(event);
        return;
      }
      if (
        !held.dragging &&
        Math.hypot(event.clientX - held.x, event.clientY - held.y) >= DRAG_DISTANCE
      ) {
        markDragging();
        suppressClick.current = true;
        draggedBefore.current = true;
        launch(dragWidget());
      }
    };
    const onRelease = (event: globalThis.PointerEvent): void => {
      if (event.pointerId !== press.current?.pointerId) {
        return;
      }
      const stillClick = !press.current.dragging && draggedBefore.current;
      finish(event);
      if (stillClick && event.type === "pointerup") {
        clickAfterDrag(event);
      }
    };
    // The system's move loop takes the button's release from the page, and the
    // first press after it can come without its click. The person did click, so
    // the click they meant is made here, unless the page's own arrives.
    const clickAfterDrag = (event: globalThis.PointerEvent): void => {
      draggedBefore.current = false;
      clickSeen.current = false;
      const under = document.elementFromPoint(event.clientX, event.clientY);
      window.setTimeout(() => {
        if (!clickSeen.current && under instanceof HTMLElement) {
          under.closest("button")?.click();
        }
      }, CLICK_GRACE_MS);
    };
    const onPress = (event: globalThis.PointerEvent): void => {
      if (event.button !== 0) {
        return;
      }
      suppressClick.current = false;
      finish();
      beginPress({
        pointerId: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        dragging: false,
      });
      window.addEventListener("pointermove", onMove, true);
      window.addEventListener("pointerup", onRelease, true);
      window.addEventListener("pointercancel", onRelease, true);
      window.addEventListener("blur", finish);
    };
    const onClick = (event: globalThis.MouseEvent): void => {
      clickSeen.current = true;
      // A click that follows a drag is swallowed, on any button. A keyboard
      // click is never part of a drag, so it is always let through.
      if (suppressClick.current && event.detail !== 0) {
        event.preventDefault();
        event.stopPropagation();
        suppressClick.current = false;
      }
    };
    window.addEventListener("pointerdown", onPress, true);
    window.addEventListener("click", onClick, true);
    return () => {
      window.removeEventListener("pointerdown", onPress, true);
      window.removeEventListener("click", onClick, true);
      finish();
    };
  }, [press, beginPress, markDragging, endPress]);
}
