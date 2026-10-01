/**
 * The snapshot subscription lifecycle.
 *
 * One subscription per webview. The Effect registers the listeners and requests
 * the snapshot once; its cleanup stops both. Registration is asynchronous, so a
 * cleanup that runs before registration settles unsubscribes the listeners the
 * moment they arrive (spec 7.8.3, AC-85).
 */
import { useEffect } from "react";

import { reconcileSnapshot, startSnapshotSubscription } from "../shared/ipc/subscription";

/**
 * Owns the webview's subscription for the lifetime of the component.
 *
 * Under Strict Mode the Effect runs, cleans up, and runs again. The store is
 * module state, not component state, so the second run does not duplicate
 * anything: the first subscription is stopped before the second starts.
 */
export function useSnapshotSubscription(): void {
  useEffect(() => {
    let stopped = false;
    let stop: (() => void) | null = null;
    const started = startSnapshotSubscription();
    started
      .then((detach) => {
        if (stopped) {
          detach();
          return;
        }
        stop = detach;
      })
      .catch(() => {
        // A registration failure leaves the renderer without events; the
        // snapshot read below still recovers the current state.
      });
    const reconciled = reconcileSnapshot();
    reconciled.catch(() => {
      // `reconcileSnapshot` already records its own failure in the store.
    });
    return () => {
      stopped = true;
      if (stop !== null) {
        stop();
      }
    };
  }, []);
}
