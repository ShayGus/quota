/**
 * Navigation between the two windows.
 *
 * The popover owns the overview and quota detail; the settings window owns
 * settings, including adding an account. When a settings control leads to a
 * popover surface, as Details does, the settings window asks the host to show
 * it and steps aside. The host creates the full window at that surface when
 * it is not open, as while the widget is the view, or else signals it.
 *
 * This is one of the audited integration wrappers allowed to use the raw event
 * API: the event is presentation routing from the host, carries no mutation,
 * and no backend listener accepts it as a command.
 */
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { commands } from "../../generated/bindings";
import type { AccountId, QuotaWindowId } from "../../generated/bindings";
import { setFailure } from "../state/store";

/** The event the popover listens for. */
const NAVIGATE_EVENT = "quota-popover-navigate";

/** A popover surface another window may ask for. */
export type PopoverTarget =
  | { readonly view: "overview" }
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

/**
 * The surface a full window was created at, from its address:
 * `#/detail/<account>` or `#/detail/<account>/<window>`, else the overview.
 */
export function initialTarget(hash: string): PopoverTarget {
  const [, view, account, quotaWindow] = hash.split("/");
  if (view !== "detail" || account === undefined || account === "") {
    return { view: "overview" };
  }
  try {
    return {
      view: "detail",
      accountId: decodeURIComponent(account),
      windowId:
        quotaWindow === undefined || quotaWindow === ""
          ? null
          : decodeURIComponent(quotaWindow),
    };
  } catch {
    return { view: "overview" };
  }
}

/** Asks the host to show one popover surface, then closes this window. */
export async function showInPopover(target: PopoverTarget): Promise<void> {
  const shown = await commands.showInOverview(
    target.view === "detail"
      ? { view: "detail", account_id: target.accountId, window_id: target.windowId }
      : { view: "overview" },
  );
  if (shown.status === "error") {
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
