/**
 * The mini widget: every account at a glance, in a small window that floats
 * above the others.
 *
 * It has two looks, picked with the same ring-or-bar choice as the overview:
 * a strip of rings, one ring per period, or mini cards with a bar per limit.
 * The window always fits its accounts exactly, so nothing is ever scrolled or
 * cut off, and it moves wherever it is dragged. Clicking an account opens it
 * in the overview; the widget itself stays where it is.
 */
import { useEffect, useMemo, useRef, type JSX, type PointerEvent } from "react";

import type { AccountId } from "../../generated/bindings";
import { launch } from "../../shared/ipc/report";
import { dragWidget, fitWidget, showInOverview } from "../../shared/ipc/widget";
import type { RendererState } from "../../shared/state/types";
import { useNow } from "../../shared/ui/useNow";
import { MiniCards } from "./MiniCards";
import { widgetAccounts } from "./model";
import { RingStrip } from "./RingStrip";

/** How far the pointer moves before a press becomes a drag, in CSS pixels. */
const DRAG_DISTANCE = 4;

/** The widget window's content. */
export function Widget({ state }: { readonly state: RendererState }): JSX.Element {
  const now = useNow();
  const accounts = useMemo(
    () => widgetAccounts(state.snapshot?.accounts ?? [], state.preferences, now),
    [state.snapshot, state.preferences, now],
  );
  const root = useRef<HTMLDivElement | null>(null);
  useFitWindow(root);
  useTransparentPage();
  const drag = useDragToMove();
  const open = (accountId: AccountId): void => {
    if (drag.consumeDrag()) {
      return;
    }
    launch(showInOverview({ view: "detail", accountId, windowId: null }));
  };
  return (
    <div
      className="widget"
      ref={root}
      onPointerDown={drag.onPointerDown}
      onPointerMove={drag.onPointerMove}
    >
      {accounts.length === 0 ? (
        <button
          type="button"
          className="widget-empty"
          onClick={() => {
            if (!drag.consumeDrag()) {
              launch(showInOverview({ view: "overview" }));
            }
          }}
        >
          <strong>No accounts to show</strong>
          <span>Open Quota to add one</span>
        </button>
      ) : state.preferences?.indicator_style === "bar" ? (
        <MiniCards accounts={accounts} onOpen={open} />
      ) : (
        <RingStrip accounts={accounts} onOpen={open} />
      )}
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
