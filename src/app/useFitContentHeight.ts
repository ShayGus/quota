/**
 * The popover's height follows its content, as the wireframe's does.
 *
 * The renderer only measures. It reports how tall its content is, and the
 * host's `fit_overview_height` command decides the window's height and
 * position inside the monitor's work area, under the window controller, so the
 * window never moves outside the host's own transitions.
 */
import { useEffect, type RefObject } from "react";

import { launch } from "../shared/ipc/report";
import { actions } from "./actions";

/** The height the popover's content asks for, in CSS pixels. */
export function contentHeight(root: HTMLElement): number | null {
  const header = root.querySelector<HTMLElement>(".app-header");
  const main = root.querySelector<HTMLElement>(".app-main");
  const footer = root.querySelector<HTMLElement>(".app-footer");
  const content = main?.firstElementChild;
  if (
    header === null ||
    main === null ||
    footer === null ||
    !(content instanceof HTMLElement)
  ) {
    return null;
  }
  return header.offsetHeight + content.scrollHeight + footer.offsetHeight;
}

/** Whether this renderer runs inside the native shell rather than a test DOM. */
function isNative(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

/** Reports the popover's content height to the host whenever it changes. */
export function useFitContentHeight(root: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const element = root.current;
    if (element === null || !isNative() || typeof ResizeObserver === "undefined") {
      return;
    }
    let frame = 0;
    let reported: number | null = null;
    const report = (): void => {
      const height = contentHeight(element);
      if (height === null || (reported !== null && Math.abs(height - reported) < 2)) {
        return;
      }
      reported = height;
      launch(actions.fitOverviewHeight(height));
    };
    const schedule = (): void => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(report);
    };
    const observer = new ResizeObserver(schedule);
    const observeAll = (): void => {
      observer.disconnect();
      observer.observe(element);
      const main = element.querySelector(".app-main");
      if (main !== null) {
        for (const child of main.children) observer.observe(child);
      }
    };
    observeAll();
    // A new surface replaces the observed content, so observe it afresh.
    const mutations = new MutationObserver(() => {
      observeAll();
      schedule();
    });
    const main = element.querySelector(".app-main");
    if (main !== null) mutations.observe(main, { childList: true });
    schedule();
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      mutations.disconnect();
    };
  }, [root]);
}
