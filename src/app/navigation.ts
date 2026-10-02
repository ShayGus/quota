/**
 * Navigation between the two windows.
 *
 * The popover owns the overview, quota detail, and add-account surfaces; the
 * settings window owns settings. When a settings control leads to a popover
 * surface, as Add account and Details do in the wireframe, the settings window
 * asks the popover to show it and steps aside.
 */
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type { AccountId, QuotaWindowId } from "../generated/bindings";

/** The event the popover listens for. */
const NAVIGATE_EVENT = "quota-popover-navigate";

/** A popover surface another window may ask for. */
export type PopoverTarget =
  | { readonly view: "overview" }
  | { readonly view: "connect" }
  | {
      readonly view: "detail";
      readonly accountId: AccountId;
      readonly windowId: QuotaWindowId | null;
    };

/** Asks the popover to show one of its surfaces, then hides this window. */
export async function showInPopover(target: PopoverTarget): Promise<void> {
  await emitTo("overview", NAVIGATE_EVENT, target);
  await getCurrentWindow().close();
}

/**
 * Listens for navigation requests in the popover. The handler shows the
 * surface; the popover brings itself forward so the request is visible.
 */
export async function listenForNavigation(
  onNavigate: (target: PopoverTarget) => void,
): Promise<() => void> {
  return listen<PopoverTarget>(NAVIGATE_EVENT, (event) => {
    onNavigate(event.payload);
    const current = getCurrentWindow();
    void current
      .show()
      .then(() => current.setFocus())
      .catch(() => undefined);
  });
}
