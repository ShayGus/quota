/**
 * The popover's height follows its content, as the wireframe's does.
 *
 * The wireframe popover has no fixed height: it grows with its cards up to
 * 760 pixels, or the space available, and scrolls beyond that. The native
 * window is sized the same way. A tray popover keeps its bottom edge where it
 * is, so it stays anchored above the taskbar; a pinned window keeps its top.
 */
import { useEffect, type RefObject } from "react";
import { LogicalSize, PhysicalPosition } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { launch } from "../shared/ipc/report";

/** The wireframe popover's tallest height. */
export const POPOVER_MAX_HEIGHT = 760;
/** The native window's minimum height, from the window configuration. */
export const POPOVER_MIN_HEIGHT = 320;

/** The height the popover's content asks for, in CSS pixels. */
export function contentHeight(root: HTMLElement): number {
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
    return POPOVER_MAX_HEIGHT;
  }
  return header.offsetHeight + content.scrollHeight + footer.offsetHeight;
}

/** The window height for some content, within the popover's bounds and the screen. */
export function fittedHeight(content: number, available: number): number {
  const ceiling = Math.max(POPOVER_MIN_HEIGHT, Math.min(POPOVER_MAX_HEIGHT, available));
  return Math.round(Math.min(ceiling, Math.max(POPOVER_MIN_HEIGHT, content)));
}

/** Whether this renderer runs inside the native shell rather than a test DOM. */
function isNative(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

/** Keeps the native popover's height fitted to its content. */
export function useFitContentHeight(
  root: RefObject<HTMLElement | null>,
  anchorBottom: boolean,
): void {
  useEffect(() => {
    const element = root.current;
    if (element === null || !isNative() || typeof ResizeObserver === "undefined") {
      return;
    }
    const native = getCurrentWindow();
    let frame = 0;
    let applying = false;
    const apply = async (): Promise<void> => {
      const target = fittedHeight(contentHeight(element), window.screen.availHeight - 48);
      if (applying || Math.abs(target - window.innerHeight) < 2) {
        return;
      }
      applying = true;
      try {
        const position = await native.outerPosition();
        const before = await native.innerSize();
        await native.setSize(new LogicalSize(window.innerWidth, target));
        if (anchorBottom) {
          const after = await native.innerSize();
          await native.setPosition(
            new PhysicalPosition(position.x, position.y + before.height - after.height),
          );
        }
      } catch {
        // A refused resize leaves the window as it was; the content scrolls.
      } finally {
        applying = false;
      }
    };
    const schedule = (): void => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        launch(apply());
      });
    };
    const observer = new ResizeObserver(schedule);
    observer.observe(element);
    const watched = element.querySelector(".app-main");
    if (watched !== null) {
      observer.observe(watched);
      for (const child of watched.children) observer.observe(child);
    }
    const mutations = new MutationObserver(() => {
      observer.disconnect();
      observer.observe(element);
      const main = element.querySelector(".app-main");
      if (main !== null) {
        observer.observe(main);
        for (const child of main.children) observer.observe(child);
      }
      schedule();
    });
    mutations.observe(element, { childList: true, subtree: true });
    schedule();
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      mutations.disconnect();
    };
  }, [root, anchorBottom]);
}
