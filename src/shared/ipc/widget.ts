/**
 * The mini widget's own window operations.
 *
 * The widget sizes itself to its accounts, so it never scrolls, moves when it
 * is dragged, and asks the overview to show an account it was clicked on. It
 * stays where it is: opening the overview never closes or moves the widget.
 *
 * This is one of the audited integration wrappers allowed to use the raw event
 * API, for the same reason as `navigation.ts`: the event is window-to-window
 * presentation routing, and no backend listener accepts it as a command.
 */
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

import { NAVIGATE_EVENT, type PopoverTarget } from "./navigation";

/** Resizes the widget window to its content, in CSS pixels. */
export async function fitWidget(width: number, height: number): Promise<void> {
  await getCurrentWindow().setSize(new LogicalSize(Math.ceil(width), Math.ceil(height)));
}

/** Starts moving the widget with the pointer that is down on it. */
export async function dragWidget(): Promise<void> {
  await getCurrentWindow().startDragging();
}

/** Asks the overview to show one of its surfaces; the overview comes forward. */
export async function showInOverview(target: PopoverTarget): Promise<void> {
  await emitTo("overview", NAVIGATE_EVENT, target);
}
