/**
 * The mini widget: every account at a glance, in a small window that floats
 * above the others.
 *
 * It has two looks, picked with the same ring-or-bar choice as the overview:
 * a strip of rings, one ring per period, or mini cards with a bar per limit.
 * It is another way to present the app, never shown beside the full window.
 * The window always fits its accounts exactly, so nothing is ever scrolled or
 * cut off, and it moves wherever it is dragged. The button in its corner, the
 * tray menu, and Settings switch back to the full window.
 */
import { useEffect, useMemo, useRef, type JSX, type PointerEvent } from "react";

import { launch } from "../../shared/ipc/report";
import { dragWidget, fitWidget } from "../../shared/ipc/widget";
import type { RendererState } from "../../shared/state/types";
import { Icon } from "../../shared/ui/Icon";
import { useNow } from "../../shared/ui/useNow";
import { MiniCards } from "./MiniCards";
import { widgetAccounts } from "./model";
import { RingStrip } from "./RingStrip";

/** How far the pointer moves before a press becomes a drag, in CSS pixels. */
const DRAG_DISTANCE = 4;

/** The widget window's content. */
export function Widget({
  state,
  onExpand,
}: {
  readonly state: RendererState;
  /** Switches back to the full window. */
  readonly onExpand: () => void;
}): JSX.Element {
  const now = useNow();
  const accounts = useMemo(
    () => widgetAccounts(state.snapshot?.accounts ?? [], state.preferences, now),
    [state.snapshot, state.preferences, now],
  );
  const root = useRef<HTMLDivElement | null>(null);
  useFitWindow(root);
  useTransparentPage();
  const drag = useDragToMove();
  const expand = (): void => {
    if (!drag.consumeDrag()) {
      onExpand();
    }
  };
  return (
    <div
      className="widget"
      ref={root}
      onPointerDown={drag.onPointerDown}
      onPointerMove={drag.onPointerMove}
    >
      {accounts.length === 0 ? (
        <button type="button" className="widget-empty" onClick={expand}>
          <strong>No accounts to show</strong>
          <span>Open the full window to add one</span>
        </button>
      ) : state.preferences?.indicator_style === "bar" ? (
        <MiniCards accounts={accounts} />
      ) : (
        <RingStrip accounts={accounts} />
      )}
      <button
        type="button"
        className="widget-expand"
        aria-label="Open the full window"
        title="Open the full window"
        onClick={expand}
      >
        <Icon name="expand" />
      </button>
    </div>
  );
}

/** Keeps the window exactly the size of its content, so it never scrolls. */
function useFitWindow(root: { readonly current: HTMLElement | null }): void {
  useEffect(() => {
    const element = root.current;
    if (element === null) {
      return;
    }
    const fit = (): void => {
      launch(fitWidget(element.offsetWidth, element.offsetHeight));
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [root]);
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
 * does not move stays a click on the account under it.
 */
function useDragToMove(): {
  readonly onPointerDown: (event: PointerEvent) => void;
  readonly onPointerMove: (event: PointerEvent) => void;
  readonly consumeDrag: () => boolean;
} {
  const press = useRef<{ x: number; y: number } | null>(null);
  const dragged = useRef(false);
  return {
    onPointerDown: (event) => {
      dragged.current = false;
      press.current = event.button === 0 ? { x: event.clientX, y: event.clientY } : null;
    },
    onPointerMove: (event) => {
      const start = press.current;
      if (start === null || (event.buttons & 1) === 0) {
        press.current = null;
        return;
      }
      if (Math.hypot(event.clientX - start.x, event.clientY - start.y) >= DRAG_DISTANCE) {
        press.current = null;
        dragged.current = true;
        launch(dragWidget());
      }
    },
    consumeDrag: () => {
      const was = dragged.current;
      dragged.current = false;
      return was;
    },
  };
}
