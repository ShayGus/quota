/**
 * The mini widget's own window operations.
 *
 * The widget sizes itself to its accounts, so it never scrolls, and moves
 * when it is dragged. The host saves where it was left.
 */
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

/** Resizes the widget window to its content, in CSS pixels. */
export async function fitWidget(width: number, height: number): Promise<void> {
  await getCurrentWindow().setSize(new LogicalSize(Math.ceil(width), Math.ceil(height)));
}

/** Starts moving the widget with the pointer that is down on it. */
export async function dragWidget(): Promise<void> {
  await getCurrentWindow().startDragging();
}
