/**
 * The mini widget: every account at a glance, in a small window that floats
 * above the others.
 *
 * It has two looks, picked with the same ring-or-bar choice as the overview:
 * a strip of rings, one ring per period, or mini cards with a bar per limit.
 * It is another way to present the app, never shown beside the full window.
 * The host fits the window's height, and it moves wherever it is dragged,
 * from any point on it. RingStrip owns the peek and details drawer. The
 * button in its corner, the tray menu, and Settings switch back to the full
 * window; beside that button, Report a bug
 * opens the same two choices as the full window's header.
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

import { launch } from "../../shared/ipc/report";
import {
  dragWidget,
  fitWidget,
  type FitWidget,
  type WidgetGrowth,
} from "../../shared/ipc/widget";
import type { RendererState } from "../../shared/state/types";
import { Icon } from "../../shared/ui/Icon";
import { TOAST_MS } from "../../shared/ui/RefreshNotice";
import {
  PROMPT_COPIED,
  ReportBugMenu,
  type ReportBugActions,
} from "../../shared/ui/ReportBug";
import { useNow } from "../../shared/ui/useNow";
import { MiniCards } from "./MiniCards";
import { widgetAccounts } from "./model";
import { RingStrip } from "./RingStrip";

/** How far the pointer moves before a press becomes a drag, in CSS pixels. */
const DRAG_DISTANCE = 4;

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

const WidgetChromeContext = createContext<WidgetChrome>({
  setOnPressEnd: (callback) => {
    defaultOnPressEnd.current = callback;
  },
  press: { current: null },
  drawerOpen: defaultDrawerOpen,
  setDrawerOpen: (open) => {
    defaultDrawerOpen.current = open;
  },
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
  report,
}: {
  readonly state: RendererState;
  /** Switches back to the full window. */
  readonly onExpand: () => void;
  /** Opens the issue form or copies the agent prompt. */
  readonly report: Omit<ReportBugActions, "onCopied">;
}): JSX.Element {
  const now = useNow();
  const accounts = useMemo(
    () => widgetAccounts(state.snapshot?.accounts ?? [], state.preferences, now),
    [state.snapshot, state.preferences, now],
  );
  const root = useRef<HTMLDivElement | null>(null);
  const chrome = useWidgetChromeValue();
  useFitWindow(root, chrome);
  useTransparentPage();
  useDragToMove(chrome);
  const [copied, showCopied] = useCopiedNotice();
  return (
    <WidgetChromeContext value={chrome}>
      <div className="widget" ref={root}>
        {accounts.length === 0 ? (
          <button type="button" className="widget-empty" onClick={onExpand}>
            <strong>No accounts to show</strong>
            <span>Open the full window to add one</span>
          </button>
        ) : state.preferences?.indicator_style === "bar" ? (
          <MiniCards accounts={accounts} />
        ) : (
          <RingStrip accounts={accounts} />
        )}
        {copied ? (
          <p className="widget-notice" role="status">
            {PROMPT_COPIED}
          </p>
        ) : null}
        <ReportBugMenu variant="widget" actions={{ ...report, onCopied: showCopied }} />
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

/** The window's shared press, drawer flag and deferred fit. */
function useWidgetChromeValue(): WidgetChrome & WidgetPressControls {
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
      target,
      requestFit,
      beginPress,
      markDragging,
      endPress,
    }),
    [requestFit, endPress, beginPress, markDragging, setDrawerOpen, setOnPressEnd],
  );
}

/**
 * Whether the copied-prompt notice is showing. The widget has no toast layer,
 * so the notice sits beneath the accounts for as long as a toast would.
 */
function useCopiedNotice(): readonly [boolean, () => void] {
  const [shownAt, setShownAt] = useState<number | null>(null);
  useEffect(() => {
    if (shownAt === null) {
      return;
    }
    const timer = window.setTimeout(() => {
      setShownAt(null);
    }, TOAST_MS);
    return () => {
      window.clearTimeout(timer);
    };
  }, [shownAt]);
  const show = (): void => {
    setShownAt(Date.now());
  };
  return [shownAt !== null, show];
}

/**
 * Observes the resting content height. While the drawer is open it fits
 * explicitly around its animation, so intermediate heights are not sent.
 */
function useFitWindow(
  root: { readonly current: HTMLElement | null },
  chrome: WidgetChrome,
): void {
  const { drawerOpen, target, requestFit } = chrome;
  useEffect(() => {
    const element = root.current;
    if (element === null) {
      return;
    }
    const fit = (): void => {
      if (drawerOpen.current || element.offsetHeight === target.current) {
        return;
      }
      launch(requestFit(element.offsetHeight, "down"));
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [root, drawerOpen, target, requestFit]);
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
        launch(dragWidget());
      }
    };
    const onRelease = (event: globalThis.PointerEvent): void => {
      if (event.pointerId === press.current?.pointerId) {
        finish(event);
      }
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
