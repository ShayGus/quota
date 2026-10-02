/**
 * Navigation between the two windows.
 *
 * The popover owns the overview, quota detail, and add-account surfaces; the
 * settings window owns settings. When a settings control leads to a popover
 * surface, as Add account and Details do in the wireframe, the settings window
 * asks the popover to show it and steps aside.
 *
 * This is one of the audited integration wrappers allowed to use the raw event
 * API: the event is window-to-window presentation routing, carries no
 * mutation, and no backend listener accepts it as a command.
 */
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type { AccountId, QuotaWindowId } from "../../generated/bindings";
import { setFailure } from "../state/store";

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

/** Records a window operation that failed, so the window can say so. */
function reportNavigationFailure(operation: string): void {
  setFailure({
    kind: "domain",
    error: {
      kind: "native_operation_failed",
      context: { operation, reason: "the window did not respond" },
    },
  });
}

/** Asks the popover to show one of its surfaces, then hides this window. */
export async function showInPopover(target: PopoverTarget): Promise<void> {
  try {
    await emitTo("overview", NAVIGATE_EVENT, target);
  } catch {
    // Without the request delivered, this window stays open for another try.
    reportNavigationFailure("navigate_popover");
    return;
  }
  await getCurrentWindow().close();
}

/**
 * Listens for navigation requests in the popover. The handler shows the
 * surface; the popover brings itself forward so the request is visible.
 */
export async function listenForNavigation(
  onNavigate: (target: PopoverTarget) => void,
): Promise<() => void> {
  try {
    return await listen<PopoverTarget>(NAVIGATE_EVENT, (event) => {
      onNavigate(event.payload);
      const current = getCurrentWindow();
      current
        .show()
        .then(() => current.setFocus())
        .catch(() => {
          reportNavigationFailure("show_popover");
        });
    });
  } catch {
    // Settings cannot route here without the listener; the popover itself
    // still works, and the failure is stated rather than hidden.
    reportNavigationFailure("listen_navigation");
    return () => undefined;
  }
}
