/**
 * The mini widget's own window operations.
 *
 * The widget requests a height for its content and moves when it is dragged.
 * The host saves where it was left. Sizing goes through the host's
 * `fit_widget` command, which owns the monitor's work area: the
 * renderer reports its content height and growth direction, and the host
 * answers with the fitted height, the side it grew, and the room on each
 * side, so the drawer can open upward near the screen's bottom edge.
 */
import { getCurrentWindow } from "@tauri-apps/api/window";

import { commands, type FitWidget, type WidgetGrowth } from "../../generated/bindings";
import { reportAsync } from "./report";

/**
 * Asks the host to fit the widget window to its content. The host applies the
 * size and, for upward growth, the position, then reports what it did. A
 * refusal is reported and answered with `null`, so the caller keeps showing
 * what it already shows.
 */
export async function fitWidget(
  contentHeight: number,
  direction: WidgetGrowth,
): Promise<FitWidget | null> {
  return reportAsync(
    commands.fitWidget(Math.max(1, Math.round(contentHeight)), direction),
  );
}

/** Starts moving the widget with the pointer that is down on it. */
export async function dragWidget(): Promise<void> {
  await getCurrentWindow().startDragging();
}

export type { FitWidget, WidgetGrowth };
